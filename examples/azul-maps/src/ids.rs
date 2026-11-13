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
    /// The microphone: the destination spoken.
    TRAVEL_SPEAK = "travel-speak";
    /// The distance once both ends are places.
    TRAVEL_DISTANCE = "travel-distance";
    /// The typeahead's suggestions under the travel fields (each row
    /// `suggestion-<n>`, see [`indexed`]).
    SUGGESTIONS = "suggestions";
    /// The line saying the ends go to the routing server, while it is asked.
    ROUTE_PRIVACY = "route-privacy";
    /// The routing server's turn-by-turn steps (each row `route-step-<n>`,
    /// see [`route_step`]).
    ROUTE_STEPS = "route-steps";
    /// The search results' credits under the suggestions.
    SUGGESTION_CREDITS = "suggestion-credits";
    /// The routing server's credit beside the Start button.
    ROUTE_CREDIT = "route-credit";
    // ---- public transport (`transit_panel.rs`) ----
    /// The public transport part of the travel panel.
    TRANSIT = "transit";
    /// Leave now / depart at / arrive by.
    TRANSIT_WHEN = "transit-when";
    TRANSIT_TIME = "transit-time";
    /// The journeys (each row `journey-<n>`); the picked one's legs (`leg-<n>`).
    JOURNEYS = "journeys";
    JOURNEY_LEGS = "journey-legs";
    TRANSIT_EARLIER = "transit-earlier";
    TRANSIT_LATER = "transit-later";
    /// The timetable service's credit and the link to its sources.
    TRANSIT_CREDIT = "transit-credit";
    TRANSIT_SOURCES = "transit-sources";
    // ---- navigation (`drive_ui.rs`) ----
    NAV_START = "nav-start";
    /// The next-maneuver banner, its distance and its instruction.
    NAV_BANNER = "nav-banner";
    NAV_DISTANCE = "nav-distance";
    NAV_INSTRUCTION = "nav-instruction";
    /// The bar: arrival, time and distance left, the switches, End.
    NAV_BAR = "nav-bar";
    NAV_SUMMARY = "nav-summary";
    NAV_HEADING = "nav-heading";
    NAV_VOICE = "nav-voice";
    NAV_CAR = "nav-car";
    NAV_END = "nav-end";
    /// The fix-quality indicator.
    GPS_INDICATOR = "gps-indicator";
    /// Settings > Navigation > Location.
    GPS_SOURCE = "gps-source";
    GPS_DEVICE = "gps-device";
    GPS_TEXT = "gps-text";
    GPS_APPLY = "gps-apply";
    /// Settings > Navigation > Routing tiles: how long downloaded roads are kept.
    ROUTING_KEEP = "routing-keep";
    /// Settings > Navigation > Offline areas: the name, Download / Stop, the line under them
    /// (the kept areas' rows are `offline-area-<n>`, their Delete `offline-delete-<n>`).
    OFFLINE_NAME = "offline-name";
    OFFLINE_DOWNLOAD = "offline-download";
    OFFLINE_STOP = "offline-stop";
    OFFLINE_NOTE = "offline-note";
    // ---- saved places (`lists_ui.rs`) ----
    /// The sidebar's saved lists (each row `list-<n>`, the open one's places `saved-place-<n>`).
    SAVED = "saved";
    SAVED_LISTS = "saved-lists";
    CLEAR_RECENT_DESTINATIONS = "clear-recent-destinations";
    /// Settings > Places.
    IMPORT_FILE = "import-file";
    IMPORT_FOLDER = "import-folder";
    IMPORT_LINK = "import-link";
    IMPORT_LINK_ADD = "import-link-add";
    IMPORT_NOTE = "import-note";
    /// The review of the places an import could not place (each `review-<n>`).
    IMPORT_REVIEW = "import-review";
    EXPORT_GPX = "export-gpx";
    EXPORT_GEOJSON = "export-geojson";
    EXPORT_NOTE = "export-note";
    // ---- the recent places ----
    /// The list of recent places (the dropped pins, newest first).
    RECENTS = "recents";
    CLEAR_PINS = "clear-pins";
    // ---- the map's controls ----
    ZOOM_IN = "zoom-in";
    ZOOM_OUT = "zoom-out";
    LOCATE = "locate";
    SETTINGS = "settings";
    // ---- markers (`Dom::with_marker`): what a callback changes in place ----
    /// The map widget's root: the navigation moves its camera and overlays in place.
    MAP_VIEW = "map-view";
    /// The banner's and the bar's text nodes the navigation rewrites in place.
    NAV_DISTANCE_TEXT = "nav-distance-text";
    NAV_INSTRUCTION_TEXT = "nav-instruction-text";
    NAV_SUMMARY_TEXT = "nav-summary-text";
    NAV_INDICATOR_TEXT = "nav-indicator-text";
    /// The banner's maneuver arrow, swapped in place for the next maneuver.
    NAV_ICON = "nav-icon";
}

/// An id made at run time: `__azmaps_<stem>-<n>` (`place-0` the first
/// place's row, `place-pin-0` its pin's icon on the map, `place-directions-0` the
/// button on its card).
#[must_use]
pub fn indexed(stem: &str, n: usize) -> AzString {
    AzString::from(format!("__azmaps_{stem}-{n}"))
}

/// A travel mode's button: `__azmaps_travel-car`, `__azmaps_travel-walk`, ...
#[must_use]
pub fn travel_mode(key: &str) -> AzString {
    AzString::from(format!("__azmaps_travel-{key}"))
}

/// A turn-by-turn step's row: `__azmaps_route-step-0`, ...
#[must_use]
pub fn route_step(n: usize) -> AzString {
    indexed("route-step", n)
}
