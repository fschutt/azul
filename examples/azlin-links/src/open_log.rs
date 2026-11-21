//! The open log: the one exception to "links are counted, never logged" (planning/architecture.md
//! section 5.12: "`open_log` | signed-in audiences only, off by default: which account or member
//! opened it and when; the recipient is told before opening (M7)"; section 5.15, M7: "link counters
//! and caps only. The one exception is the open log (5.12): signed-in audiences only, off by
//! default, and shown to the recipient before opening").
//!
//! Three rules live here and in `count_open`:
//!
//! - **off by default and for signed-in audiences only**: `validate` refuses `open_log` on an
//!   `anyone` link (S16c1), and an anonymous caller never produces an entry (`entry_for`);
//! - **the recipient is told before opening**: `resolve` says so (`Resolved::tell_recipient_open_log`,
//!   from `is_logged`), and `count_open` refuses to count an open of a logged link until the
//!   recipient's app has shown the notice and sends the acknowledgement;
//! - **an entry is an account, a member and a time, and nothing else**: no device, no address (the
//!   crate has no address type at all, section 5.15 R1/R2).
//!
//! The entry is RETURNED to the node (`Opened::log_entry`); where the open log is kept and how long
//! is the node's (it is not a column of `reg_link`, which holds counters only).
//!
//! Implements planning/architecture.md section 5.12 (`open_log`) and section 5.15 (M7).

use azlin_seams::{AccountId, MemberId, Stamp};

use crate::id::PublicLinkId;
use crate::record::LinkRecord;
use crate::resolve::Caller;

/// One line of the open log: which account and member opened the link, and when ("which account or
/// member opened it and when").
///
/// Implements planning/architecture.md section 5.12 (`open_log`) and section 5.15 (M7).
///
/// Caller: `count_open` (the human step builds it); Seam: LINKS-NODE (it appends the entry to the
/// link's open log and shows the sharer the list).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OpenLogEntry {
    /// The link that was opened.
    pub link: PublicLinkId,
    /// The account of the member who opened it.
    pub account: AccountId,
    /// The member who opened it.
    pub member: MemberId,
    /// The leader's stamp of the open.
    pub at: Stamp,
}

/// Whether the link keeps an open log, and so whether the recipient must be told before opening.
///
/// Implements planning/architecture.md section 5.12 (`open_log`: "the recipient is told before
/// opening") and section 5.15 (M7).
///
/// Caller: `resolve` (`Resolved::tell_recipient_open_log`), `entry_for`; Seam: the apps (they show
/// the notice and send the acknowledgement of `count_open`).
pub fn is_logged(r: &LinkRecord) -> bool {
    r.options().open_log
}

/// The log entry of an open by `caller` at `at`: only for a link that keeps an open log and only for
/// a signed-in caller (an anonymous caller has neither an account nor a member to name, and
/// `validate` never lets an `anyone` link keep a log).
///
/// Implements planning/architecture.md section 5.12 (`open_log`: "signed-in audiences only") and
/// section 5.15 (M7).
///
/// Caller: `count_open`.
pub fn entry_for(r: &LinkRecord, caller: &Caller, at: Stamp) -> Option<OpenLogEntry> {
    if !is_logged(r) {
        return None;
    }
    match caller {
        Caller::Anonymous => None,
        Caller::SignedIn { account, member }
        | Caller::Member {
            account, member, ..
        } => Some(OpenLogEntry {
            link: *r.id(),
            account: *account,
            member: *member,
            at,
        }),
    }
}
