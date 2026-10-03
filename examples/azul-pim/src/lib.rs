//! azul-pim: the personal-information logic the Azlin apps share.
//!
//! AzCalendar, AzTasks, AzContacts and AzMail each grew their own copy of the
//! same small pieces (scripts/DEDUP_EDITORS_2026_10_02.md, C1): month math,
//! repeat rules, the content lines of `.ics` and `.vcf` files, address lines,
//! search, initials, and two task files that claimed one layout. They live
//! here once, as plain Rust (no azul dependency, like `azul-storage`), with
//! the tests of every copy they replace.
//!
//! - [`dates`]: days in a month, month steps (strict for RRULE, clamped for a
//!   to-do), week starts, weekday / month names and codes, ordinals.
//! - [`rrule`]: the RFC 5545 RRULE subset AzCalendar reads and writes, and
//!   the dates a rule makes.
//! - [`repeat`]: a to-do's repeat ("monthly on the 31st", "3 days after
//!   completion"), the next occurrence, and the RRULE it is.
//! - [`content_line`]: iCalendar / vCard content lines - folding at 75
//!   octets, TEXT escaping, parameters, groups.
//! - [`mail_address`]: address lines (`"Lovelace, Ada" <ada@example.org>,
//!   bo@example.org`), the address in an entry, address checks.
//! - [`search`]: "every word, any case, diacritics folded" matching.
//! - [`initials`]: an avatar's initials from a name.
//! - [`task`] and [`task_store`]: the one task store - `tasks/<list>/<task
//!   uuid>.json` - every app reads and writes, through `azul-storage`'s
//!   `Drive`, with the migration of AzCalendar's old To-Do bar files.
//! - [`data_uri`]: `data:` URIs and standard base64 both ways (a contact's
//!   photo, a mail's inline image).
//! - [`write_queue`]: the write-behind queue the apps' durable writes go
//!   through (one write per key, one batch in flight, failures kept for a
//!   retry), run as a batch on a `Drive` from a file thread.

pub mod content_line;
pub mod data_uri;
pub mod dates;
pub mod initials;
pub mod mail_address;
pub mod repeat;
pub mod rrule;
pub mod search;
pub mod task;
pub mod task_store;
pub mod write_queue;

/// A temporary folder for tests: this crate's, and the apps' through the `test-util` feature.
#[cfg(any(test, feature = "test-util"))]
pub mod testing;
