use azul::widgets::PieTone as MarkerTone;

use super::*;

/// A pie says its percent while the sync works and for the drive's space; a sync at rest is its
/// colour alone.
#[test]
fn a_pie_says_its_percent_while_it_works_and_for_the_drives_space() {
    let busy = sync_pie(Some("azdrive-sync-status-syncing-files"), Some(0.42));
    assert_eq!(pie_text(&busy, false), "42 %");
    let done = sync_pie(Some("azdrive-sync-status-up-to-date"), None);
    assert_eq!(pie_text(&done, false), "", "a sync at rest is its colour alone");
    assert_eq!(pie_text(&space_pie(50, 100), true), "50 %");
    assert_eq!(pie_text(&space_pie(10, 0), true), "0 %", "an unknown size is an empty disc");
}

/// A pie at work breathes (the widget's Busy tone); the others have the colour their state has.
#[test]
fn a_pie_at_work_breathes_and_the_states_have_their_colours() {
    let busy = sync_pie(Some("azdrive-sync-status-syncing"), Some(0.5));
    assert_eq!(tone_of(&busy), MarkerTone::Busy);
    assert_eq!(tone_of(&space_pie(10, 100)), MarkerTone::Accent);
    assert_eq!(tone_of(&space_pie(90, 100)), MarkerTone::Warning);
    assert_eq!(tone_of(&space_pie(99, 100)), MarkerTone::Danger);
    let done = sync_pie(Some("azdrive-sync-status-up-to-date"), None);
    assert_eq!(tone_of(&done), MarkerTone::Good);
}
