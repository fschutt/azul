//! The top-edge rule of a list that has a Preview (the wireframe's row 12, the user's sentence of
//! 2026-11-28): while the list is SCROLLED the SELECTION and the PREVIEW both follow the row at the
//! list's TOP EDGE, so that a reader scrolls through a folder of photos or a book of contacts and
//! stops where the Preview shows the right one; an explicit tap, click or key CHOICE holds the
//! selection until the list is scrolled on.
//!
//! Plain data, no azul types: the app hands the list's rows as `Option<K>` (`Some(key)` is a row that
//! holds an item - a contact, a task, a message; `None` is a header that holds none: a letter, a
//! date group, a section), the first row in view and the rows in view, as the list reports them (a
//! `SummaryList`'s `Scroll` event, or the app's own scroll box measured against its row height), and
//! gets back whether the selection and the Preview stay or go to another item.
//!
//! The state a list keeps is [`ListScroll`]. Three rules sit in it, each with the case that made it:
//!
//! - a choice (`chose`) pins the top row it was made at, and the pin holds until the top edge has
//!   moved [`PIN_SLACK`] rows: a list that nudges itself by one row to keep the chosen row in view
//!   would otherwise take the choice away at once;
//! - a choice of a row OUTSIDE the rows in view (the arrow keys went past the edge) makes the next
//!   scroll report the selection's own (`swallow`): the list scrolls the row into view and that is
//!   not the reader's scroll;
//! - a list nobody has scrolled yet counts every row as in view.
//!
//! The same rule served AzMail first (`nav::follow`, a copy of it); every app after it calls this.

/// What a report of the list's scroll does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Follow<K> {
    /// Nothing changes: a choice holds the selection, the scroll was the selection's own, or the
    /// item at the top edge is the one shown.
    Keep,
    /// The selection goes to row `row`, the Preview to the item `key`.
    Move { row: usize, key: K },
}

/// How many rows the list's top edge may move after a choice before the scroll counts as the
/// reader's own: a list that nudges itself to keep the chosen row in view moves it by one.
pub const PIN_SLACK: usize = 2;

/// Whether the Preview follows the list's top edge now: always, except while a choice holds it
/// (`pinned_top`: the top row at the choice) and the top edge has not moved past the slack.
#[must_use]
pub fn follows_top_edge(pinned_top: Option<usize>, top: usize) -> bool {
    pinned_top.is_none_or(|pinned| top.abs_diff(pinned) >= PIN_SLACK)
}

/// The item at the top edge of a list whose first row in view is `first`: that row when it holds
/// one, else the next row below it that does (a header holds none); its row and its key.
#[must_use]
pub fn top_item<K: Copy>(rows: &[Option<K>], first: usize) -> Option<(usize, K)> {
    rows.iter()
        .enumerate()
        .skip(first)
        .find_map(|(row, item)| item.map(|key| (row, key)))
}

/// The effect of a scroll report: the list's first row in view is `first`; `pinned_top` is the top
/// row at the last choice while that choice holds; `shown` is the item the Preview shows now.
#[must_use]
pub fn follow<K: Copy + PartialEq>(
    rows: &[Option<K>],
    first: usize,
    pinned_top: Option<usize>,
    shown: Option<K>,
) -> Follow<K> {
    if !follows_top_edge(pinned_top, first) {
        return Follow::Keep;
    }
    match top_item(rows, first) {
        Some((row, key)) if shown != Some(key) => Follow::Move { row, key },
        _ => Follow::Keep,
    }
}

/// What the app keeps of a list's scroll: the rows in view as the list last reported them, the
/// choice that holds the selection, and whether the next scroll report is the selection's own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListScroll {
    /// The first row in view; `end` the row after the last one (`usize::MAX`: not reported yet,
    /// every row counts as in view).
    pub top: usize,
    pub end: usize,
    /// The top row at the last click, tap or key while that choice holds.
    pub pinned_top: Option<usize>,
    /// The choice was a row outside the rows in view (the keys went past the edge): the list will
    /// scroll it into view, and that scroll is the selection's, not the reader's.
    pub swallow: bool,
}

impl ListScroll {
    /// The state of a list nobody has scrolled or chosen from.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            top: 0,
            end: usize::MAX,
            pinned_top: None,
            swallow: false,
        }
    }

    /// A row was chosen (a click, a tap, a key): it holds the selection until the list is scrolled
    /// on, and if it is out of view its scroll into view is not the reader's.
    pub fn chose(&mut self, row: usize) {
        self.pinned_top = Some(self.top);
        self.swallow = row < self.top || row >= self.end;
    }

    /// The list reports that rows `start..end` are in view: what the selection and the Preview do.
    pub fn scrolled<K: Copy + PartialEq>(
        &mut self,
        rows: &[Option<K>],
        start: usize,
        end: usize,
        shown: Option<K>,
    ) -> Follow<K> {
        let own = core::mem::take(&mut self.swallow);
        self.top = start;
        self.end = end;
        if own {
            // the choice holds at the place the list scrolled to
            self.pinned_top = Some(start);
            return Follow::Keep;
        }
        let effect = follow(rows, start, self.pinned_top, shown);
        if matches!(effect, Follow::Move { .. }) {
            self.pinned_top = None;
        }
        effect
    }

    /// The list's content changed under the reader (another folder, a search, a sort): the choice
    /// and the report of the old list mean nothing for the new one.
    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

impl Default for ListScroll {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "follow_test.rs"]
mod follow_test;
