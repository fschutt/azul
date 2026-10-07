//! AzMaps' DOM ids, each name defined ONCE with the app's `__azmaps_` prefix
//! (user ruling 2026-10-02).

use azul::str::String as AzString;

macro_rules! names {
    ($($(#[$doc:meta])* $name:ident = $value:literal;)*) => {
        $($(#[$doc])* pub const $name: AzString = AzString::from_const_str(concat!("__azmaps_", $value));)*
    };
}

names! {
    /// The map's area: the whole window (the widget and what is drawn on it).
    MAP = "map";
    /// The draggable title area over the map.
    TITLE = "title";
    /// The sidebar's show / hide button (in the title area).
    SIDEBAR_TOGGLE = "sidebar-toggle";
    /// The sidebar: the travel panel and the recent places.
    SIDEBAR = "sidebar";
    // ---- the travel panel ----
    TRAVEL = "travel";
    TRAVEL_FROM = "travel-from";
    TRAVEL_TO = "travel-to";
    TRAVEL_SWAP = "travel-swap";
    /// The distance once both ends are places.
    TRAVEL_DISTANCE = "travel-distance";
    // ---- the recent places ----
    /// The list of recent places (the dropped pins, newest first).
    RECENTS = "recents";
    CLEAR_PINS = "clear-pins";
    // ---- the map's controls ----
    ZOOM_IN = "zoom-in";
    ZOOM_OUT = "zoom-out";
    LOCATE = "locate";
    SETTINGS = "settings";
}

/// An id made at run time: `__azmaps_<stem>-<n>` (`place-0` the first
/// place's row, `place-pin-0` its pin, `place-directions-0` the button in its
/// popover).
#[must_use]
pub fn indexed(stem: &str, n: usize) -> AzString {
    AzString::from(format!("__azmaps_{stem}-{n}"))
}

/// A travel mode's button: `__azmaps_travel-car`, `__azmaps_travel-walk`, ...
#[must_use]
pub fn travel_mode(key: &str) -> AzString {
    AzString::from(format!("__azmaps_travel-{key}"))
}
