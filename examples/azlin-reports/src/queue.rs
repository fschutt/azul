//! The order of the Reports page and the daily report, and what is overdue.
//!
//! Implements planning/architecture.md section 3.9: "Reports for CSAM, terrorism or a threat to life head the
//! Reports page and the daily report; an ordinary report is decided within 7 days **est.**" (a "threat to life"
//! is not on the fixed list of reasons: drift X-DELTA-W1S46A-3).
//!
//! Caller: S46-INT-3 (AzCtl's Reports page and `azctl account content list` order by [`order`]; the daily
//! report lists [`overdue`]).

use core::cmp::Ordering;

use crate::body::Reason;
use crate::ids::{Day, Days, ReportId};

/// An ordinary report is decided within this many days (planning/architecture.md section 3.9: 7 days, **est.**).
pub const ORDINARY_DAYS: Days = Days::new(7);

/// One row of the Reports page, as the queue sees it.
///
/// Caller: S46-INT-3 (AzCtl builds one per report it has opened).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueueItem {
    /// The report's receipt.
    pub id: ReportId,
    /// The reporter's reason.
    pub reason: Reason,
    /// The day the report was received.
    pub received: Day,
    /// Whether the operator has decided it.
    pub decided: bool,
}

/// When a report is due.
///
/// Caller: S46-INT-3 (the Reports page shows it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Due {
    /// At once: a CSAM or terrorism report (section 3.9: they head the page).
    Immediately,
    /// By the end of this day: an ordinary report, `received + 7 days`.
    By(Day),
}

/// The total order of the page: urgent reasons first, then the oldest report first, then by id; two rows that
/// are equal in all of that are ordered by reason and by decided flag, so the order is TOTAL and the result does
/// not depend on the order the rows came in.
fn compare(a: &QueueItem, b: &QueueItem) -> Ordering {
    b.reason
        .is_urgent()
        .cmp(&a.reason.is_urgent())
        .then_with(|| a.received.cmp(&b.received))
        .then_with(|| a.id.cmp(&b.id))
        .then_with(|| a.reason.cmp(&b.reason))
        .then_with(|| a.decided.cmp(&b.decided))
}

/// Sorts the rows of the Reports page: urgent reasons (`Csam`, `Terrorism`) first, then oldest first, then by
/// id. Total and stable: the same set of rows gives the same order whatever order it came in.
///
/// Caller: S46-INT-3.
pub fn order(items: &mut [QueueItem]) {
    items.sort_by(compare);
}

/// When `item` is due: `Immediately` for the urgent reasons, `received + 7 days` for the others.
///
/// Caller: S46-INT-3.
pub fn due(item: &QueueItem) -> Due {
    if item.reason.is_urgent() {
        Due::Immediately
    } else {
        Due::By(item.received.plus(ORDINARY_DAYS))
    }
}

/// The undecided items that are past due on `today` (drift X-DELTA-W1S46A-17), in the order of the page. An
/// `Immediately` item is overdue from the day it was received; a `By(d)` item on every day after `d` (it is
/// still on time on `d` itself).
///
/// Caller: S46-INT-3 (the daily report).
pub fn overdue(items: &[QueueItem], today: Day) -> Vec<ReportId> {
    let mut rows = items.to_vec();
    order(&mut rows);
    rows.iter()
        .filter(|item| !item.decided)
        .filter(|item| match due(item) {
            Due::Immediately => today >= item.received,
            Due::By(day) => today > day,
        })
        .map(|item| item.id)
        .collect()
}
