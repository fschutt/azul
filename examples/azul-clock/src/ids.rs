//! AzClock's DOM ids and classes, each name defined ONCE, every one carrying
//! the app's `__azclock_` prefix (user ruling 2026-10-02, like the widgets'
//! `__azul_`): no clash with a widget's or another app's names, and no string
//! copied at run time for a fixed name.

use azul::str::String as AzString;

/// The prefix of every name.
pub const PREFIX: &str = "__azclock_";

macro_rules! names {
    ($($(#[$doc:meta])* $name:ident = $value:literal;)*) => {
        $($(#[$doc])* pub const $name: AzString = AzString::from_const_str(concat!("__azclock_", $value));)*
    };
}

names! {
    // ---- the mode row ----
    /// The World / Alarms / Timer / Stopwatch switch.
    MODES = "modes";
    /// "+": a new alarm, timer or city.
    NEW = "new";
    SETTINGS = "settings";
    /// The line under the content: the next alarm, a notice.
    STATUS = "status";
    NOTICE = "notice";
    TOAST = "toast";
    // ---- world ----
    WORLD_VIEW = "world-view";
    FACE = "face";
    /// Classes of the face's hands.
    HAND_HOUR = "hand-hour";
    HAND_MINUTE = "hand-minute";
    HAND_SECOND = "hand-second";
    FACE_TIME = "face-time";
    CITIES = "cities";
    /// Class of one city row.
    CITY_ROW = "city-row";
    CITY_MODAL = "city-modal";
    CITY_QUERY = "city-query";
    CITY_RESULTS = "city-results";
    /// Class of one search result.
    CITY_RESULT = "city-result";
    // ---- alarms ----
    ALARMS_VIEW = "alarms-view";
    ALARMS = "alarms";
    /// Class of one alarm row, and of its switch.
    ALARM_ROW = "alarm-row";
    ALARM_SWITCH = "alarm-switch";
    EDITOR_MODAL = "editor-modal";
    EDITOR = "editor";
    EDITOR_TIME = "editor-time";
    EDITOR_REPEAT = "editor-repeat";
    EDITOR_LABEL = "editor-label";
    EDITOR_SOUND = "editor-sound";
    EDITOR_SNOOZE = "editor-snooze";
    EDITOR_DELETE = "editor-delete";
    EDITOR_CANCEL = "editor-cancel";
    EDITOR_SAVE = "editor-save";
    // ---- timer ----
    TIMER_VIEW = "timer-view";
    TIMER_TIME = "timer-time";
    /// The time left as a bar (TODO(WIDGETS9B): the Gauge's ring).
    TIMER_RING = "timer-ring";
    PRESETS = "presets";
    TIMERS = "timers";
    /// Class of one timer row.
    TIMER_ROW = "timer-row";
    // ---- stopwatch ----
    STOPWATCH_VIEW = "stopwatch-view";
    STOPWATCH_BOX = "stopwatch-box";
    /// The MARKER of the stopwatch's text node, retexted 30 times a second.
    STOPWATCH_TIME = "stopwatch-time";
    LAPS = "laps";
    /// Class of one lap row.
    LAP_ROW = "lap-row";
    // ---- ringing ----
    RING_MODAL = "ring-modal";
    RING = "ring";
    RING_SNOOZE = "ring-snooze";
    RING_DISMISS = "ring-dismiss";
    // ---- the clock's settings ----
    SET_RING_MINUTES = "set-ring-minutes";
}

/// A name made at run time from a part defined once elsewhere (an alarm's
/// or a city's index): the prefix, then `suffix`.
#[must_use]
pub fn named(suffix: &str) -> AzString {
    AzString::from(format!("{PREFIX}{suffix}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_carries_the_app_prefix() {
        for name in [MODES, NEW, SETTINGS, STATUS, NOTICE, STOPWATCH_TIME, EDITOR_SAVE, RING_DISMISS] {
            assert!(name.as_str().starts_with(PREFIX), "{}", name.as_str());
        }
        assert_eq!(named("alarm-0").as_str(), "__azclock_alarm-0");
    }
}
