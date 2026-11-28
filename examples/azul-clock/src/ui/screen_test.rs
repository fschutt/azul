//! The four places of the bottom bar: their order, their keys (a saved setting and `--screen`
//! name them) and their words. No window.

use super::Screen;

#[test]
fn the_bottom_bar_runs_alarm_clock_timer_stopwatch() {
    let keys: Vec<&str> = Screen::ALL.iter().map(|s| s.key()).collect();
    assert_eq!(keys, ["alarms", "world", "timer", "stopwatch"]);
}

#[test]
fn a_places_index_is_its_position_in_the_bar_and_its_key_finds_it() {
    for (i, screen) in Screen::ALL.iter().enumerate() {
        assert_eq!(screen.index(), i);
        assert_eq!(Screen::by_key(screen.key()), Some(*screen));
    }
    assert_eq!(Screen::by_key("settings"), None, "settings is a page, not a place");
}

#[test]
fn the_clock_place_keeps_the_key_older_builds_saved() {
    // `world` is what a build before the bottom bar wrote to settings.json and what `--screen` takes.
    assert_eq!(Screen::by_key("world"), Some(Screen::World));
    assert_eq!(Screen::World.label(), "azclock-tab-clock");
}

#[test]
fn every_place_has_its_own_word() {
    let labels: std::collections::BTreeSet<&str> = Screen::ALL.iter().map(|s| s.label()).collect();
    assert_eq!(labels.len(), Screen::ALL.len());
}
