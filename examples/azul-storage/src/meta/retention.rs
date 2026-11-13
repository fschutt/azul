//! How long a drive keeps what it no longer shows (ARCH28 RF44 / AR5a-5; X-AR5a-21).
//!
//! Retention had four owners that disagreed - the UI's restore window (14 days), the packs' grace
//! (30 days), the sync engine's trash (30 days) and the index's own garbage grace (a day) - so a
//! file the user could see in the history had lost its bytes, or the other way round. The spans a
//! user is promised are here; the window of the UI, the grace of a retired pack, the reclaimer and
//! both trashes (the sync's and AzDrive's: `TRASH_DAYS`, `KEPT_DAYS`) read them from this one
//! place.
//!
//! The index's own garbage grace ([`super::wal::Maintenance::grace`]) is something else: how long
//! an object of the repository itself, no manifest names, waits for a slow reader. It is not a
//! promise to the user.

use std::time::Duration;

/// The spans a drive keeps what it released.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Retention {
    /// How long a deleted file stays in the trash - the sync's trash folder and AzDrive's own, one
    /// span for both (the owner's decision Q18, 30 days): they were two literals, and this field
    /// said fourteen.
    pub trash: Duration,
    /// How far back a plain drive whose NODE keeps the replaced objects itself can be restored (the
    /// node's own retention, asked of the token server): what "Restore as of..." offers on a build
    /// without encrypted drives. Not a span of the client's.
    pub node: Duration,
    /// How long an encrypted drive keeps the versions of its files: an object the drive's history
    /// no longer names in its head stays in the bucket until its last commit is this old. Neither
    /// span counts toward the quota (owner's decision of 2026-10-24).
    pub versions: Duration,
}

/// What the drives keep: the trash 30 days (Q18), a plain drive's node 14, the versions 90 days (the
/// owner's decision (a)).
pub const RETENTION: Retention = Retention {
    trash: Duration::from_secs(30 * 86_400),
    node: Duration::from_secs(14 * 86_400),
    versions: Duration::from_secs(90 * 86_400),
};

impl Retention {
    /// The trash span in whole days.
    #[must_use]
    pub const fn trash_days(self) -> u64 {
        self.trash.as_secs() / 86_400
    }

    /// The node's span in whole days.
    #[must_use]
    pub const fn node_days(self) -> u64 {
        self.node.as_secs() / 86_400
    }

    /// The versions span in whole days.
    #[must_use]
    pub const fn versions_days(self) -> u64 {
        self.versions.as_secs() / 86_400
    }

    /// The oldest `committer.time` (seconds since 1970) a commit may carry for what it replaced or deleted
    /// to still be offered by a restore at `now`: [`Retention::versions`] before it. THE derivation of the
    /// restore window (X-DELTA-SYNC4-8): the restore dialog (azcloud-kit `restore::file_versions`) and the
    /// reclaimer's check of its own window (`meta::reclaim::restorable`) both ask it, where each used to
    /// write the subtraction and the comparison its own way; a pair test runs both over one repository
    /// with a merge, a skewed commit and a restore.
    #[must_use]
    pub fn restore_window_start(self, now: u64) -> i64 {
        i64::try_from(now.saturating_sub(self.versions.as_secs())).unwrap_or(i64::MAX)
    }

    /// Whether a version that the commit made at `replaced_at` replaced (or deleted) is offered by a restore
    /// at `now`: a version is offered while the change that REPLACED it is inside the window.
    #[must_use]
    pub fn offers(self, replaced_at: i64, now: u64) -> bool {
        replaced_at >= self.restore_window_start(now)
    }
}
