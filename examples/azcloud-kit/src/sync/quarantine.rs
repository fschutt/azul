//! Quarantine marks for other people's files. A file that came down from a share participant
//! (not from one of the user's own devices) gets the system's "downloaded from elsewhere"
//! mark, so the system checks it before it runs: `com.apple.quarantine` on macOS (Gatekeeper),
//! the `Zone.Identifier` stream ("Mark of the Web") on Windows (SmartScreen). Linux has no such
//! check, so nothing is set there. The mark itself is `azul_storage::quarantine` (AzDrive's own
//! Open and Download set it too); here is which writers need it.
//!
//! The mark goes on after the download is renamed in and before the pass reports it. A file
//! that cannot be marked (a disk without extended attributes) is still synced; the pass lists
//! that once as a warning.

pub use azul_storage::quarantine::{
    is_marked, macos_value, mark, mark_downloaded, supported, AGENT, ZONE_IDENTIFIER,
};

/// Who wrote a file, as the quarantine rule sees it: the user's own devices need no mark.
pub trait Writers: Sync {
    /// Whether `device` (a remote entry's writer) is one of the user's own devices.
    fn is_own(&self, device: &str) -> bool;
}

/// The user's own devices by name (the command line's `--own-devices`; an app passes its
/// account's devices).
#[derive(Clone, Debug, Default)]
pub struct OwnDevices(pub Vec<String>);

impl Writers for OwnDevices {
    fn is_own(&self, device: &str) -> bool {
        self.0.iter().any(|d| d == device)
    }
}
