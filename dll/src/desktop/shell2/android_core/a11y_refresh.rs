//! When the accessibility snapshot is rebuilt after frames that changed no layout.
//!
//! TalkBack and `uiautomator` read the bounds of every node from the snapshot the loop published after the last
//! LAYOUT. A scroll moves every node under a scroll container without a layout (the shell only repaints), so the
//! published bounds stayed those of the unscrolled page: a dump after a scroll listed a key at y 2116 that was drawn 96
//! px higher (the demo pipeline's AzCompanion run 45787048273: `ui-after-tap.xml` byte-identical to the dump before), and
//! a user who explores by touch was told the wrong thing about what is under the finger.
//!
//! Rebuilding the snapshot at every repaint frame would walk the whole tree 60 times a second during a scroll. This
//! decides when a rebuild is worth it: once the repaints have been quiet for [`QUIET_MS`] (a flick has ended, a finger has
//! lifted), and in any case every [`MAX_STALE_MS`] while they go on, so a dump taken in the middle of a long flick is
//! behind by a bounded amount. Time is a plain millisecond count of the caller's clock: nothing in here reads one.

/// How long the repaints must have stopped before the snapshot is rebuilt.
pub const QUIET_MS: u64 = 120;

/// The longest the published snapshot stays behind a scroll that goes on.
pub const MAX_STALE_MS: u64 = 500;

/// What the published snapshot has not seen: the repaints since the last time it was built.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScrollRefresh {
    /// When the first repaint the snapshot has not seen happened, `None` when it has seen them all.
    stale_since: Option<u64>,
    /// When the last repaint happened.
    last_repaint: u64,
}

impl ScrollRefresh {
    /// A snapshot that has seen everything.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            stale_since: None,
            last_repaint: 0,
        }
    }

    /// A frame was repainted from the existing layout (a scroll offset, an animation, a hover style): the bounds the
    /// snapshot carries may be out of date.
    pub fn note_repaint(&mut self, now_ms: u64) {
        if self.stale_since.is_none() {
            self.stale_since = Some(now_ms);
        }
        self.last_repaint = now_ms;
    }

    /// A snapshot was published (by a layout regeneration or by the rebuild this asked for): what came before it is seen.
    pub fn note_rebuilt(&mut self) {
        self.stale_since = None;
    }

    /// Whether the snapshot should be rebuilt now: something is unseen and the repaints have been quiet for
    /// [`QUIET_MS`] or the snapshot has been behind for [`MAX_STALE_MS`].
    #[must_use]
    pub fn is_due(&self, now_ms: u64) -> bool {
        let Some(since) = self.stale_since else {
            return false;
        };
        let quiet = now_ms >= self.last_repaint && now_ms - self.last_repaint >= QUIET_MS;
        let behind = now_ms.saturating_sub(since) >= MAX_STALE_MS;
        quiet || behind
    }
}

#[cfg(test)]
#[path = "a11y_refresh_test.rs"]
mod tests;
