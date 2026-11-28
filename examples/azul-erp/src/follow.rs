//! The Preview and the selection follow the row at the TOP EDGE of the table while it scrolls (the
//! user's rule of the refactor, wireframe row 12: scroll through a long list and stop where the
//! Preview shows the record you want); a click, a tap or a key on a row overrides it until the table
//! is scrolled on. The policy is plain data and plain arithmetic, in POSITIONS (the n-th row of the
//! rows SHOWN: what the table's sort and filters left, `DataTableView::top`), so it is tested
//! without a window; `ui/table.rs` feeds it the table's reports.
//!
//! The model is AzMail's list (`nav::ListScroll` there), with the table's own words: the table
//! reports `top`, the first position in view, and the app asks what that does.

/// How many positions the top edge may move after a choice before the scroll counts as the
/// reader's own: a table that nudges itself to keep the chosen row in view moves it by one.
pub const PIN_SLACK: u32 = 2;

/// What a report of the table's scroll asks of the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Nothing: a choice holds the selection, or the row at the top edge is the one shown.
    Keep,
    /// The selection and the Preview move to the row at position `position`.
    Select(u32),
}

/// What the window keeps of the table's scroll.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Follow {
    /// The first position in view, as the table last reported it.
    pub top: u32,
    /// The top edge at the last choice, while that choice holds.
    pub pinned_top: Option<u32>,
}

impl Follow {
    /// A table nobody has scrolled or chosen from: the first row is at the top edge.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            top: 0,
            pinned_top: None,
        }
    }

    /// A row was chosen (a click, a tap, a key, a row opened): it holds the selection until the
    /// table is scrolled on. `top` is where the table's top edge is now (a key that moved past the
    /// edge scrolled the table with it: that scroll is the choice's own).
    pub fn chose(&mut self, top: u32) {
        self.top = top;
        self.pinned_top = Some(top);
    }

    /// The table reports that its first position in view is `top`; `shown` is the position the
    /// Preview shows now (`None`: no row is shown). What the selection and the Preview do.
    pub fn scrolled(&mut self, top: u32, shown: Option<u32>) -> Effect {
        self.top = top;
        if !follows_top_edge(self.pinned_top, top) {
            return Effect::Keep;
        }
        // the reader scrolled past the slack: the choice no longer holds
        self.pinned_top = None;
        if shown == Some(top) {
            Effect::Keep
        } else {
            Effect::Select(top)
        }
    }

    /// The table was built again over other rows (a new page, a record kind): a choice made on
    /// the old rows holds nothing, and the top edge is the first row again.
    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

/// Whether the Preview follows the top edge now: always, except while a choice holds it
/// (`pinned_top`: the top edge at the choice) and the edge has not moved past [`PIN_SLACK`].
#[must_use]
pub fn follows_top_edge(pinned_top: Option<u32>, top: u32) -> bool {
    pinned_top.is_none_or(|pinned| top.abs_diff(pinned) >= PIN_SLACK)
}

/// The position the first build of a table selects: the row at the top edge, when there is one.
#[must_use]
pub const fn first_selection(rows_shown: u32) -> Option<u32> {
    if rows_shown == 0 {
        None
    } else {
        Some(0)
    }
}

#[cfg(test)]
#[path = "follow_test.rs"]
mod follow_test;
