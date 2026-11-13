//! azul-calendar-core: AzCalendar's events without azul types.
//!
//! AzCalendar (examples/azul-calendar) links libazul for its windows; these modules run without
//! it, so a headless process - the Azlin Bridge's CalDAV - reads and writes the same files with
//! the same code. AzCalendar re-exports them under the names it always had (`crate::event`).
//!
//! - [`event`]: one event is one JSON file, `events/<uuid>.json`.
//! - [`calendars`]: the calendars, `calendars/<id>.json`.
//! - [`ics`]: iCalendar (RFC 5545) import and export.
//! - [`meet_rooms`]: AzMeet's meeting links (AzMeet's own `rooms.rs`, compiled in by path, as
//!   AzCalendar did).

pub mod calendars;
pub mod event;
pub mod ics;
pub use azul_pim::rrule;

/// AzMeet's meeting links and room keys: AzMeet's own `rooms.rs`, compiled in here too, so the
/// apps read links the same way.
#[path = "../../azul-meet/src/rooms.rs"]
pub mod meet_rooms;

// A temporary folder for tests (the one the PIM apps share).
#[cfg(test)]
use azul_pim::testing as test_dir;
