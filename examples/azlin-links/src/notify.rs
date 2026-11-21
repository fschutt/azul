//! Telling the sharer: the `notify` option of planning/architecture.md section 5.12 ("`notify` |
//! never, the first open, every open; delivered as `link_used` (5.13)"). The event is `link_used`
//! with the payload "link, downloads" (section 5.13, the table of events).
//!
//! This module decides ONE thing: whether an open is told to the sharer, and what the event says. It
//! delivers nothing (the event feed is S33's: `link_used` is one of the events dropped first under
//! pressure, so a missed one costs the sharer a line of news and nothing else). `count_open` calls
//! it at the human step ("an open counts only after the human step", section 5.12 "Scanners and
//! previews"): a scanner that only resolves a link never tells the sharer anything.
//!
//! Implements planning/architecture.md section 5.12 (`notify`) and section 5.13 (`link_used`).

use crate::id::PublicLinkId;
use crate::options::Notify;

/// The event `link_used` of the event table: the link, and the number of complete downloads it has
/// counted when the open happened. Counters and caps only: it names no one (section 5.15, M7).
///
/// Implements planning/architecture.md section 5.13 (`link_used`: link, downloads) and section 5.15
/// (M7: counters only).
///
/// Caller: `count_open` (the human step builds it); Seam: S33 (it delivers the event to the
/// sharer's devices).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LinkUsed {
    /// The link that was opened.
    pub link: PublicLinkId,
    /// The complete downloads counted so far.
    pub downloads: u64,
}

/// Whether an open is told to the sharer: `never` tells nothing, `first_open` tells the first open
/// of the link only, `every_open` tells each one.
///
/// Implements planning/architecture.md section 5.12 (`notify`: "never, the first open, every open").
///
/// Caller: `link_used`.
pub fn tells_sharer(notify: Notify, first_open: bool) -> bool {
    match notify {
        Notify::Never => false,
        Notify::FirstOpen => {
            first_open || azlin_seams::mutant!("links_notify_first_open_every_time")
        }
        Notify::EveryOpen => true,
    }
}

/// The event for an open, if the link's `notify` option says the sharer is told: `link` is the
/// opened link, `first_open` whether this open is the first one the link counts, `downloads` the
/// complete downloads counted when it happened.
///
/// Implements planning/architecture.md section 5.12 (`notify`) and section 5.13 (`link_used`).
///
/// Caller: `count_open`; Seam: S33 (it delivers the event).
pub fn link_used(
    notify: Notify,
    link: PublicLinkId,
    first_open: bool,
    downloads: u64,
) -> Option<LinkUsed> {
    if tells_sharer(notify, first_open) {
        Some(LinkUsed { link, downloads })
    } else {
        None
    }
}
