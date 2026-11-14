//! Azlin reports: content reports, the counting that pauses a link, decisions and their statements of reasons
//! (planning/architecture.md section 3.9, 5.3, 5.12 and 7.10; build unit S46, lane W1-S46a).
//!
//! The pure logic around a content report, in azul because both repos build it: the report dialog of AzDrive
//! builds the body, AzCtl (azul-apps) opens it. No I/O, no clock, no randomness, no sealing:
//!
//! - [`body`]: the REPORT BODY the reporter's app seals to the operator's report key and AzCtl opens: a fixed
//!   [`Reason`], optional text and contact (never required: Art. 16(2) DSA), a [`Subject`] (a link with its key,
//!   chat records with their authors' signatures, a member, a file), strict deterministic CBOR with its limits.
//!   [`ReportSealer`] and [`RecordVerifier`] are the seams (sealing is AZD2's, signatures are azlin-keys').
//! - [`ledger`]: the COUNTING ENGINE of the automatic steps: three different devices pause a link, five
//!   different devices within 30 days disable an account's public links (`links_disabled`), counted from opaque
//!   ids and days alone, bounded, once per breach, restarted by the operator's decision.
//! - [`queue`]: the order of the Reports page (CSAM and terrorism first, then oldest first) and the due dates
//!   (seven days for an ordinary report).
//! - [`decision`]: the operator's [`Decision`] and its validation against the report.
//! - [`notice`]: the [`StatementOfReasons`] (DSA Art. 17) for the affected account and the [`NotifierNotice`]
//!   (Art. 16(5)) for the notifier, as inbox notices (7.10).
//! - [`block`]: the [`BlockList`] of "Blocking members" (Apple guideline 1.2).
//!
//! The ids are opaque ([`ReportId`], [`LinkRef`], [`AccountRef`], [`DeviceRef`]) and the one unit of time is the
//! [`Day`]. No type of this crate holds an address, a name, a payer or an ip: the only free personal text is the
//! reporter's own optional `contact`, which no `Debug` prints.
//!
//! ```
//! use azlin_reports::{
//!     AutoStep, AccountRef, Config, Day, DeviceRef, Extras, LinkRef, Ledger, Reason, ReportBody,
//!     ReportEvent, ReportId, Subject, Text,
//! };
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // The reporter's app builds the body and encodes it (then seals it to the operator's key).
//! let body = ReportBody {
//!     id: ReportId::from_bytes([7; 16]),
//!     reason: Reason::Phishing,
//!     text: Some(Text::new("pretends to be my bank")?),
//!     contact: None,
//!     subject: Subject::Link {
//!         url: Text::new("https://drive.azlin.io/l/abc123#the-share-key")?,
//!         extras: Extras::default(),
//!     },
//!     reply: None,
//!     extras: Extras::default(),
//! };
//! let bytes = body.encode()?;
//! assert_eq!(ReportBody::decode(&bytes)?, body);
//!
//! // The leader counts: the third different device pauses the link.
//! let link = LinkRef::from_bytes([1; 16]);
//! let mut ledger = Ledger::new(Config::default());
//! let mut steps = Vec::new();
//! for device in 0..3u8 {
//!     steps = ledger.record(ReportEvent {
//!         report: ReportId::from_bytes([device; 16]),
//!         link: Some(link),
//!         account: AccountRef::from_bytes([2; 16]),
//!         reporter: DeviceRef::from_bytes([device; 16]),
//!         day: Day::new(20_000),
//!     })?;
//! }
//! assert_eq!(steps, vec![AutoStep::PauseLink { link, devices: 3 }]);
//! # Ok(())
//! # }
//! ```

#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable,
        clippy::todo,
        clippy::unimplemented,
        clippy::indexing_slicing
    )
)]
#![warn(missing_docs)]

pub mod block;
pub mod body;
pub mod decision;
pub mod ids;
pub mod ledger;
pub mod notice;
pub mod queue;

pub use block::{BlockError, BlockList, Contact, BLOCK_VERSION, MAX_BLOCKED};
pub use body::{
    BodyError, Extras, ExtrasError, Field, Reason, RecordVerifier, ReportBody, ReportSealer,
    SealError, SignedRecord, Subject, Text, BODY_VERSION, MAX_BODY, MAX_CONTACT, MAX_RECORD,
    MAX_RECORDS, MAX_REPLY, MAX_TEXT, MAX_URL,
};
pub use decision::{Action, Decision, DecisionError};
pub use ids::{AccountRef, Day, Days, DeviceRef, LinkRef, ReportId};
pub use ledger::{AutoStep, Bound, Config, Ledger, LedgerError, ReportEvent, Stats};
pub use notice::{
    statement_of, statement_of_step, Facts, Ground, NotifierNotice, Outcome, Redress, Restriction,
    StatementOfReasons,
};
pub use queue::{due, order, overdue, Due, QueueItem, ORDINARY_DAYS};
