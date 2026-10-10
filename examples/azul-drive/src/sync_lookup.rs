//! What the search asks the sync about a drive's files - the seam SYNC17's per-file sync state
//! store implements ([`SyncLookup`]; until it is in, [`NoSync`]):
//!
//! - where a file's plain copy on this computer is: a cloud or encrypted drive's index reads a
//!   file's text there (an encrypted drive's never from the bucket, unless "Index files that
//!   are not downloaded" is on), and a result's line comes from it;
//! - a file's sync state, which a result shows (OneDrive's Status column in Explorer).
//!
//! Both answer from memory: the search calls them on the UI thread while it draws the results
//! and on the index's worker thread, never waiting on the network.

use std::path::PathBuf;

/// A file's sync state as a result shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncState {
    /// In the cloud only: opening it downloads it.
    OnlineOnly,
    /// On this computer, as in the cloud.
    OnThisDevice,
    /// Being uploaded or downloaded.
    Syncing,
    /// It could not be synced (a conflict, an error): the sync says why.
    Problem,
}

impl SyncState {
    /// The Status cell: its icon and its words (a key of the resources).
    #[must_use]
    pub fn badge(self) -> (&'static str, &'static str) {
        match self {
            SyncState::OnlineOnly => ("cloud_queue", "azdrive-sync-lookup-online-only"),
            SyncState::OnThisDevice => ("check_circle", "azdrive-sync-lookup-on-device"),
            SyncState::Syncing => ("sync", "azdrive-sync-lookup-syncing"),
            SyncState::Problem => ("sync_problem", "azdrive-sync-lookup-problem"),
        }
    }
}

/// The sync's answers for the search, from memory.
pub trait SyncLookup: Send + Sync {
    /// The file on this computer holding the plain (decrypted) contents of `key` of the drive
    /// `drive_id` as they are now; `None`: no copy here (online only, or not synced).
    fn local_copy(&self, drive_id: &str, key: &str) -> Option<PathBuf>;

    /// The sync state of `key` of the drive `drive_id`; `None` where nothing syncs.
    fn sync_state(&self, drive_id: &str, key: &str) -> Option<SyncState>;

    /// Whether anything syncs on this computer: the results show a Status column then.
    fn syncs(&self) -> bool {
        true
    }
}

/// No sync on this computer: no copies, no states.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoSync;

impl SyncLookup for NoSync {
    fn local_copy(&self, _drive_id: &str, _key: &str) -> Option<PathBuf> {
        None
    }

    fn sync_state(&self, _drive_id: &str, _key: &str) -> Option<SyncState> {
        None
    }

    fn syncs(&self) -> bool {
        false
    }
}
