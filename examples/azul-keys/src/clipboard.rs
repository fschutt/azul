//! The clipboard guard: a copied secret is cleared from the clipboard after a while (30 s by
//! default, 0 = never). The guard keeps only a SHA-256 digest of what was copied and a label
//! ("password of CodeHost") - never the value - so the status bar can count down and a later
//! copy can restart the countdown.
//!
//! azul cannot read the clipboard outside a paste, so the guard cannot tell whether another
//! program put something there since; when the countdown ends it clears the clipboard (a value
//! copied elsewhere meanwhile is cleared too - the plan's known gap).

use std::fmt;

use sha2::{Digest, Sha256};

/// The default seconds before a copied secret is cleared.
pub const DEFAULT_CLEAR_SECONDS: u64 = 30;

/// What was copied, as a digest, and when.
#[derive(Clone, Default)]
pub struct ClipboardGuard {
    digest: Option<[u8; 32]>,
    label: String,
    copied_at: u64,
    /// Seconds before the clipboard is cleared (0 = never).
    pub clear_after: u64,
}

impl fmt::Debug for ClipboardGuard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClipboardGuard")
            .field("pending", &self.digest.is_some())
            .field("label", &self.label)
            .field("copied_at", &self.copied_at)
            .field("clear_after", &self.clear_after)
            .finish()
    }
}

impl ClipboardGuard {
    #[must_use]
    pub fn new(clear_after: u64) -> ClipboardGuard {
        ClipboardGuard {
            clear_after,
            ..ClipboardGuard::default()
        }
    }

    /// `value` was copied at `now` (seconds); `label` says what it is.
    pub fn copied(&mut self, value: &str, label: &str, now: u64) {
        self.digest = Some(digest_of(value));
        self.label = label.to_string();
        self.copied_at = now;
    }

    /// Seconds until the clipboard is cleared; `None` when nothing is waiting (or never cleared).
    #[must_use]
    pub fn remaining(&self, now: u64) -> Option<u64> {
        if self.digest.is_none() || self.clear_after == 0 {
            return None;
        }
        Some((self.copied_at + self.clear_after).saturating_sub(now))
    }

    /// Whether the clipboard is due to be cleared at `now`.
    #[must_use]
    pub fn due(&self, now: u64) -> bool {
        self.remaining(now) == Some(0)
    }

    /// Whether the guarded value is `value`.
    #[must_use]
    pub fn holds(&self, value: &str) -> bool {
        self.digest == Some(digest_of(value))
    }

    /// The clipboard was cleared (or the guard is dropped on lock): nothing is waiting.
    pub fn cleared(&mut self) {
        self.digest = None;
        self.label.clear();
    }

    /// What was copied ("" when nothing waits).
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }
}

/// The digest the guard keeps.
fn digest_of(value: &str) -> [u8; 32] {
    let mut out = [0u8; 32];
    out.copy_from_slice(&Sha256::digest(value.as_bytes()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_copy_counts_down_and_is_due_after_the_timeout() {
        let mut guard = ClipboardGuard::new(30);
        assert_eq!(guard.remaining(100), None);
        assert!(!guard.due(100));
        guard.copied("q7#Rt!vW2m", "password of CodeHost", 100);
        assert_eq!(guard.remaining(100), Some(30));
        assert_eq!(guard.remaining(121), Some(9));
        assert!(!guard.due(129));
        assert!(guard.due(130));
        assert_eq!(guard.remaining(500), Some(0));
        assert_eq!(guard.label(), "password of CodeHost");
    }

    #[test]
    fn a_later_copy_restarts_the_countdown_with_its_label() {
        let mut guard = ClipboardGuard::new(30);
        guard.copied("a", "user name of Shop", 100);
        guard.copied("b", "one-time code of Mail", 120);
        assert_eq!(guard.remaining(125), Some(25));
        assert_eq!(guard.label(), "one-time code of Mail");
        assert!(guard.holds("b") && !guard.holds("a"));
    }

    #[test]
    fn clearing_ends_the_countdown_and_zero_seconds_never_clears() {
        let mut guard = ClipboardGuard::new(30);
        guard.copied("a", "x", 1);
        guard.cleared();
        assert_eq!(guard.remaining(2), None);
        assert!(!guard.due(1000));
        assert_eq!(guard.label(), "");
        let mut never = ClipboardGuard::new(0);
        never.copied("a", "x", 1);
        assert_eq!(never.remaining(2), None);
        assert!(!never.due(1_000_000));
    }

    #[test]
    fn the_guard_keeps_only_a_digest_of_the_secret() {
        let mut guard = ClipboardGuard::new(30);
        guard.copied("hunter2-secret", "password of Forum", 1);
        let shown = format!("{guard:?}");
        assert!(!shown.contains("hunter2"), "{shown}");
        assert!(guard.holds("hunter2-secret"));
        assert!(!guard.holds("hunter2-Secret"));
    }
}
