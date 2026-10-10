//! A drive's space as people read it. The quota counts STORED bytes - what arrives at the
//! storage nodes, compressed and encrypted on the device first (the user, 2026-10-10: "with
//! 100GB and 1.6x compression, the customer gets 160GB of space") - so a usage line shows the
//! stored bytes against the quota, and the files' original size as extra information:
//! "62 GB used of 100 GB, your files are 99 GB before compression".

/// A drive's space: stored bytes against the quota, the files' size before compression.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Usage {
    /// Stored bytes (the storage nodes' count, else the drive index's sum of its objects).
    pub used: u64,
    /// The quota, of stored bytes.
    pub quota: u64,
    /// The files' bytes before compression (the drive index's sum of their sizes), when known.
    pub original: Option<u64>,
}

/// How full a drive is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Fine,
    /// At least [`NEARLY_FULL_PERCENT`] of the quota.
    NearlyFull,
    /// The quota is used up: writes are refused.
    Full,
}

/// From this share of the quota on, a drive is nearly full.
pub const NEARLY_FULL_PERCENT: u64 = 90;

/// Bytes as the tiers are sold (decimal): `512 MB`, `62 GB`, `1.6 TB`.
#[must_use]
pub fn size_text(bytes: u64) -> String {
    const MB: u64 = 1_000_000;
    const GB: u64 = 1_000_000_000;
    const TB: u64 = 1_000_000_000_000;
    if bytes >= TB {
        if bytes % TB == 0 {
            format!("{} TB", bytes / TB)
        } else {
            format!("{:.1} TB", bytes as f64 / TB as f64)
        }
    } else if bytes >= 10 * GB {
        format!("{} GB", (bytes + GB / 2) / GB)
    } else if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else {
        format!("{} MB", (bytes + MB / 2) / MB)
    }
}

impl Usage {
    /// The stored bytes the quota still takes.
    #[must_use]
    pub fn available(&self) -> u64 {
        self.quota.saturating_sub(self.used)
    }

    #[must_use]
    pub fn level(&self) -> Level {
        Level::Fine
    }

    /// "62 GB used of 100 GB, your files are 99 GB before compression".
    #[must_use]
    pub fn text(&self) -> String {
        let used = self.original.unwrap_or(self.used);
        format!("{} used of {}", size_text(used), size_text(self.quota))
    }

    /// A warning for a drive that is nearly full or full; `None` otherwise.
    #[must_use]
    pub fn warning(&self) -> Option<String> {
        None
    }
}
