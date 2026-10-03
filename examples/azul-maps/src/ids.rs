//! AzMaps' DOM ids, each name defined ONCE with the app's `__azmaps_` prefix
//! (user ruling 2026-10-02).

use azul::str::String as AzString;

macro_rules! names {
    ($($(#[$doc:meta])* $name:ident = $value:literal;)*) => {
        $($(#[$doc])* pub const $name: AzString = AzString::from_const_str(concat!("__azmaps_", $value));)*
    };
}

names! {
    /// The map's area (the widget and its overlays).
    MAP = "map";
    /// The pins pane.
    PINS = "pins";
    // ---- the toolbar ----
    PAN_LEFT = "pan-left";
    PAN_RIGHT = "pan-right";
    PAN_UP = "pan-up";
    PAN_DOWN = "pan-down";
    ZOOM_IN = "zoom-in";
    ZOOM_OUT = "zoom-out";
    RECENTRE = "recentre";
    LOCATE = "locate";
    CLEAR_PINS = "clear-pins";
    SETTINGS = "settings";
}
