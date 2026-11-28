//! The follow-scroll mode of the folder view (a phone's): while the list scrolls, the SELECTION
//! and the PREVIEW both follow the row at its top edge - scroll through a folder of photos and stop
//! exactly where the preview shows the right one. A tap (a click, a key, a check box) chooses a row
//! by itself and PINS it: nothing moves it until the list scrolls again, and the next scroll
//! releases it, the top-edge row being the selected one again.
//!
//! What the mode leaves alone:
//! - the keyboard: an arrow key moves the selection and scrolls the list to show it - a scroll the
//!   app made itself ([`Follow::app_scrolled`]) is not the user's and moves nothing;
//! - the check boxes: a selection of several rows (rows checked one by one) is the user's choice
//!   and a scroll never takes it away; the mode is for one row at a time;
//! - every action on the selection (Delete, Move to, Rename ...): it acts on the selection as it
//!   is, which is the row the user sees highlighted, and the undo list is the same.
//!
//! The selection follows at once (a row passing the top edge is cheap); the preview's own content -
//! a picture, a PDF page - is asked for when the scroll has settled ([`load_due`]), and the pane
//! keeps the picture it had meanwhile, so a fast scroll makes no flicker of loading notices.
//!
//! A desktop's wheel does not move the selection: the mode is a touch layout's ([`follows_scroll`],
//! the one place that says so). This module holds the rules, without azul types, so that they can
//! be tested; `ui_preview::follow_scroll` is what runs them from the view's scroll callback.

/// Whether the folder view follows its scroll in a layout: a phone's does, a desktop's wheel and
/// scroll bar scroll the list and leave the selection alone.
#[must_use]
pub(crate) const fn follows_scroll(phone: bool) -> bool {
    phone
}

/// How far (px) the scroll offset must move to be a scroll: smaller than a hair's breadth, a report
/// of the offset it had is not one.
const MOVED_PX: f32 = 0.5;

/// How far (px) a scroll event may lie from where the app scrolled the list to itself and still be
/// that scroll.
const OWN_PX: f32 = 1.5;

/// What a scroll asks of the selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    /// The selection stays.
    Nothing,
    /// The row with this key is the selection now.
    Select(String),
}

/// What the folder view's scroll remembers between its reports.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Follow {
    /// The offset of the last report.
    last: f32,
    /// Where the app scrolled the list to itself, until the report of it came (a key's reveal, a
    /// folder opening at its top).
    own: Option<f32>,
}

impl Follow {
    /// The app scrolled the list to `offset` itself: the report of that scroll is no one's choice.
    pub(crate) fn app_scrolled(&mut self, offset: f32) {
        if offset.is_finite() {
            self.own = Some(offset);
        }
    }

    /// The list reports its scroll offset `offset` with the row `top` at its top edge (`None`: no
    /// row), while `selected` is the selection. What the selection does.
    ///
    /// Only a scroll moves it: a report of the offset the list already had (the end of a gesture
    /// that moved nothing, the tap that came before) is a pinned row's, and the app's own scrolls
    /// are skipped.
    pub(crate) fn scrolled(&mut self, offset: f32, top: Option<&str>, selected: &[&str]) -> Step {
        if !offset.is_finite() {
            return Step::Nothing;
        }
        if let Some(own) = self.own {
            if (own - offset).abs() <= OWN_PX {
                self.own = None;
                self.last = offset;
                return Step::Nothing;
            }
            // the list is somewhere else than the app put it: the user took it from there
            self.own = None;
        }
        if (offset - self.last).abs() < MOVED_PX {
            return Step::Nothing;
        }
        self.last = offset;
        after_scroll(selected, top)
    }
}

/// The selection after the list scrolled so that `top` is at its top edge: the top row alone - from
/// nothing, from another row (a tap's pin the scroll releases) - and nothing at all when several
/// rows are selected (check boxes) or the top row is the selection already.
#[must_use]
pub(crate) fn after_scroll(selected: &[&str], top: Option<&str>) -> Step {
    let Some(top) = top else {
        return Step::Nothing;
    };
    match selected {
        [] => Step::Select(top.to_string()),
        [one] if *one == top => Step::Nothing,
        [_one] => Step::Select(top.to_string()),
        _ => Step::Nothing,
    }
}

/// The item the preview shows: the first selected row - the one row a scroll or a tap chose, the first
/// of the rows checked - and, while nothing is selected, the first row the view shows.
#[must_use]
pub(crate) fn subject<T>(first_selected: Option<T>, first_shown: Option<T>) -> Option<T> {
    first_selected.or(first_shown)
}

/// Whether the preview's own content is asked for now: when the scroll has settled. While the list
/// runs the selection and the pane's stage follow every row; the picture of a row it passes is
/// never read.
#[must_use]
pub(crate) const fn load_due(settled: bool) -> bool {
    settled
}

#[cfg(test)]
#[path = "follow_test.rs"]
mod follow_test;
