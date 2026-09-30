//! AzCalendar: a small calendar that mints AzMeet links.

pub mod event;
pub mod meeting;
pub mod week;

/// AzMeet's meeting links and room keys: AzMeet's own `rooms.rs`, compiled into AzCalendar too,
/// so the two apps read links the same way.
#[path = "../../azul-meet/src/rooms.rs"]
pub mod meet_rooms;

pub fn start() {
    eprintln!("[azcalendar] not built yet");
}
