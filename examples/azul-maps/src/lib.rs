//! AzMaps: the `MapWidget` (layout/src/widgets/map.rs) in an app - the map is
//! the window, everything else floats on it.
//!
//! ```text
//! +----------------------------------------------------------------------+
//! | o o o  [sidebar]           (drag the window here)         [settings] |  title area
//! |  +- sidebar ---------+                                               |
//! |  | o  From      [^v] |          the map, the whole window            |
//! |  | v  To             |                     v                         |
//! |  | [car][walk][bike] |                  +--------+                   |
//! |  |   [transit] 412km |                  | popover|  a place's card   |
//! |  | Recents     [del] |                  +--------+          [locate] |
//! |  | v 37.77 N 122.4 W |                                       [+] [-] |
//! |  +-------------------+                         (c) OpenStreetMap ... |
//! +----------------------------------------------------------------------+
//! ```
//!
//! A click on the map drops a pin; the pins are the RECENT places (newest
//! first in the sidebar, a click centres the map on one and opens its card).
//! A place's card shows above its pin (Directions, Remove): the pin marker's
//! popup. The travel panel takes a start (empty: where you are, once the
//! location is known) and a destination, each a place name picked from the
//! typeahead's suggestions (`places.rs`, searched on the geocoder worker once
//! the typing rests) or `lat, lon`, a mode (drive, walk, cycle, public
//! transport), and draws the way between them: the straight line with its
//! distance at once, then the road route the route worker works out on the
//! routing tiles (`--routing`, ROUTING.md), or its estimate without them.
//!
//! Pins are kept in the data tree (`maps/pins.json`, through azul-appkit's
//! file jobs on a Thread); the last viewport and the sidebar in settings.json.
//! azul-appkit gives the switches (`--theme`, `--mode`, `--size`, `--shot`,
//! `--data-dir`) and the settings page (the gear, Mod+,). Arrows pan, `+` /
//! `-` zoom (not while a travel field is being typed in). The tiles come from
//! `--tiles <url template>`, else `AZMAPS_TILES`, else the shared Azlin
//! config's `endpoints.tiles` (a local tile server for an offline run), else
//! the map widget's own (OpenFreeMap's public planet tiles). Downloaded tiles
//! are kept in the data folder (`maps/tiles/`) as long as the tile server says
//! they are fresh, so a restart shows them without a download; the world at
//! zoom 0 and 1 ships in the binary (`assets/seed/`), so the map shows the
//! world before any download and offline.
//!
//! stdout, for scripts (`scripts/azmaps_e2e.py`): `AZMAPS_VIEW <lat> <lon>
//! <zoom>` on every viewport change, `AZMAPS_PINS <n>` when the pins change,
//! `AZMAPS_PINS_LOADED <n>` / `AZMAPS_PINS_SAVED <n>` / `AZMAPS_PINS_ERROR`,
//! `AZMAPS_PLACE <index>` when a place's card opens, `AZMAPS_TRAVEL <mode>
//! <from> <to>` when the travel panel changes, `AZMAPS_SUGGEST <from|to> <n>
//! <compute_ms>` when the typeahead's suggestions come back from the geocoder
//! worker, `AZMAPS_ROUTE <mode> <km>
//! <minutes> <compute_ms>` when its route comes back from the route worker,
//! `AZMAPS_SIDEBAR open|closed`. With `--stats` (`args.rs`): `AZMAPS_LAYOUT
//! <n> <us>` for every window rebuild (how long `layout` took), `AZMAPS_PAN
//! map <us>` for every step of a pan (how long AzMaps took for it), and the map widget's `AZ_MAP_TILES` /
//! `AZ_MAP_RENDER` / `AZ_MAP_TILE` counters.
//!
//! A navigator too: public transport journeys (`transit_panel.rs`, Transitous), turn-by-turn
//! navigation of a road route with spoken prompts, re-routing and arrival (`drive_ui.rs`,
//! `navigate.rs`, `voice.rs`), the position from the platform or an external GPS receiver
//! (`location.rs`), saved places in GeoJSON lists with imports from GPX, KML, CSV and Google
//! Takeout (`lists_ui.rs`, `saved.rs`, `import/`), and one cache of every online answer so that
//! a route, a journey or a place asked again needs no network (`services.rs`). More stdout for
//! scripts (`scripts/azmaps_nav_e2e.py`): `AZMAPS_SUGGEST_SOURCE <end> <n>
//! local|online|offline`, `AZMAPS_ROUTE_ORIGIN <mode> online|offline`, `AZMAPS_JOURNEYS <n>
//! <departure> <arrival> <changes> <origin>` and `AZMAPS_LEG <kind> <line>`, `AZMAPS_NAV_START`,
//! `AZMAPS_NAV <next step> <to it m> <left m> <state> <lat,lon>`, `AZMAPS_SAY <text>`,
//! `AZMAPS_REROUTE` / `AZMAPS_REROUTED <n> <points> <origin>`, `AZMAPS_ARRIVED`,
//! `AZMAPS_GPS <lat,lon> <quality>`, `AZMAPS_LISTS <lists> <places>`, `AZMAPS_IMPORTED <lists>
//! <places> <placed> <review>`.
//!
//! Nothing here blocks the UI thread: the tiles are fetched, decoded AND drawn
//! on the map's workers, the pins file and settings.json go through
//! azul-appkit's file thread, a route is worked out on the route worker
//! (`route.rs`), a place name is looked up on the geocoder worker
//! (`geocode.rs`). A keystroke in a travel field is the field's own edit: the
//! window is rebuilt only for the suggestions that come back. A pan or a zoom
//! does not rebuild the window: what AzMaps puts at places (the pins, the
//! travel line and its ends, where you are) are the map widget's overlays,
//! which it draws in its own view and moves with every pan, pinch, turn and
//! tilt.

pub mod args;
mod drive_ui;
pub mod geocode;
pub mod ids;
pub mod import;
pub mod journeys;
mod lists_ui;
mod l10n;
#[cfg(test)]
mod l10n_tests;
pub mod location;
pub mod model;
pub mod navigate;
mod offline_ui;
pub mod places;
pub mod route;
pub mod saved;
pub mod services;
mod transit_panel;
pub mod voice;

use std::{
    cell::Cell,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
};

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, TextInputOnTextInputCallbackType,
        TextInputOnVirtualKeyDownCallbackType, TimerCallbackInfo, TimerCallbackReturn,
    },
    css::{ColorU, CssProperty, CssPropertyWithConditions, DarkLightMode, LayoutWidth, PixelValue, SystemColorRef},
    dom::{DomId, GeolocationProbeConfig, VirtualKeyCode},
    option::OptionString,
    prelude::*,
    sensor::SensorKind,
    shells::{ShellThemeAccent, ShellThemeScope},
    str::String as AzString,
    task::{TerminateTimer, Thread, ThreadId},
    time::{Duration, SystemTimeDiff},
    vec::{MapLatLonVec, MapOverlayVec, U8Vec},
    widgets::{
        Button, ButtonType, MapLatLon, MapLineDash, MapMarker, MapOverlay, MapPolyline, MapSetup, MapTileId,
        MapTileLayer, MapViewport, MapWidget, OnTextInputReturn, TextInputState, TextInputValid,
    },
};
use azul_appkit::{
    about::AboutInfo,
    args::AppSpec,
    files::{FileJob, FileOutcome},
    l10n::{label, t_args, Arg},
    shortcuts::Shortcut,
    ui as kit,
};

use crate::{
    geocode::{SearchReply, SearchRequest},
    model::{
        cardinal, distance_km, distance_text, duration_text, fit_view, pan_tiles, parse_place,
        parse_view, pin_of_marker, pins_from_json, pins_to_json, plan_overlays, End, FROM_MARKER,
        HERE_MARKER, TO_MARKER,
        place_text, route_line as route_reply_line, travel_line, view_line, view_value, TravelMode, HOME, MAX_ZOOM,
        MIN_ZOOM, PINS_FILE, SIDEBAR_KEY, VIEW_KEY,
    },
    places::{Suggestion, SuggestionSource},
    route::{route_source, Online, RouteKind, RouteProgress, RouteReply, RouteRequest, RouteSource, Tiles},
};

/// What azul-appkit's switches know about AzMaps.
pub const SPEC: AppSpec = AppSpec {
    name: "AzMaps",
    binary: "AzMaps",
    summary: "a map with pins, on azul's MapWidget",
    screens: &["map"],
    files_help: "",
};

/// The About facts.
pub const ABOUT: AboutInfo = AboutInfo {
    name: "AzMaps",
    version: env!("CARGO_PKG_VERSION"),
    summary: "A map with pins on azul's MapWidget: OpenStreetMap vector tiles, your pins kept \\
              in your data folder.",
    license: "MIT",
    app_folder: "maps",
};

/// The keys AzMaps answers (the kit adds Mod+, / F1 / Escape).
pub const SHORTCUTS: [Shortcut; 6] = [
    Shortcut::new("azmaps-shortcuts-map", "Left", "azmaps-pan-west"),
    Shortcut::new("azmaps-shortcuts-map", "Right", "azmaps-pan-east"),
    Shortcut::new("azmaps-shortcuts-map", "Up", "azmaps-pan-north"),
    Shortcut::new("azmaps-shortcuts-map", "Down", "azmaps-pan-south"),
    Shortcut::new("azmaps-shortcuts-map", "+", "azmaps-zoom-in"),
    Shortcut::new("azmaps-shortcuts-map", "-", "azmaps-zoom-out"),
];

/// AzMaps' own categories of the settings page (`azmaps-category-<name>`): Navigation (where
/// the position comes from), Places (imports, the review, exports).
pub const APP_CATEGORIES: [&str; 2] = ["Navigation", "Places"];

/// The write-back tags of AzMaps' file jobs.
const TAG_LOAD: u64 = 1;
const TAG_SAVE: u64 = 2;

/// How often the sensor / location / keep-the-viewport timer runs. It had no
/// interval: it ran on every frame, and every magnetometer reading rebuilt the
/// window.
const TICK_MS: u64 = 100;

/// Timer ticks without a viewport change before the viewport is kept (a
/// drag or a held key is one write, not one per frame): half a second.
const VIEW_SAVE_IDLE_TICKS: u32 = 5;

/// `--stats`: the window rebuilds and the pan steps on stdout
/// (`AZMAPS_LAYOUT <n> <us>`, `AZMAPS_PAN <what> <us>`).
static STATS: AtomicBool = AtomicBool::new(false);
static LAYOUTS: AtomicU64 = AtomicU64::new(0);

/// How long the typing in a travel field rests before its text is searched:
/// a search per keystroke would only be superseded by the next one.
const TYPEAHEAD_REST_MS: u128 = 250;

/// The zoom a single picked place is shown at: its town.
const PICKED_PLACE_ZOOM: f32 = 11.0;

/// The pin's head: a 20 px square, round but for its bottom-left corner,
/// turned 45 degrees COUNTER-clockwise so that corner points straight down.
/// (It was `rotate(45deg)`: CSS turns clockwise, which put the point on
/// the left - every pin lay on its side.) The point is 10 * sqrt(2) px
/// below the square's centre.
const PIN_HEAD: &str = "width: 20px; height: 20px; border-radius: 10px 10px 10px 0px; background: \
                        #e5322d; transform: rotate(-45deg); box-shadow: 0px 1px 3px \
                        rgba(0,0,0,0.45); display: flex; align-items: center; justify-content: \
                        center; cursor: pointer; @theme(flora) { background: #7E4A42; box-shadow: \
                        0px 1px 3px rgba(48, 45, 38, 0.45); @media (prefers-color-scheme: dark) { \
                        background: #B3837A; box-shadow: 0px 1px 3px rgba(0, 0, 0, 0.55); } }";
const PIN_DOT: &str = "width: 7px; height: 7px; border-radius: 4px; background: #ffffff; \
                       @theme(flora) { background: #F4F2EA; }";
/// The pin's point in its box, the marker's anchor: half its width from the
/// left, 10 * sqrt(2) px below the head's centre.
const PIN_HALF_WIDTH: f32 = 10.0;
const PIN_TIP_DEPTH: f32 = 24.1;

/// The window's root: the positioning box everything floats in.
const ROOT: &str =
    "position: relative; flex-grow: 1; min-height: 0px; overflow: hidden; display: flex;";
/// The map: the whole window. Ground under the tiles; the tiles follow the
/// mode themselves.
///
/// Under flora (`@theme(flora)` blocks after the flat values here and below; flat is
/// unchanged) the ground is flora's desk, the floating panels are leaves at flora's corners
/// under its warm shadow, a section head is flora's label, the licence line and the toast
/// are flora's paper and clay. The map's marks are flora's too, not Google's red and blue: a
/// pin and the destination in flora's clay (its glow at night), the start, the travel line and
/// where you are in the theme's accent (a spin's stone, the glow at night).
const MAP_AREA: &str = "position: absolute; left: 0px; top: 0px; right: 0px; bottom: 0px; \
                        background: #cbd2d8; overflow: hidden; @theme(flora) { background: \
                        system:under-page-background; }";
/// The title area over the map: the window's drag region. The window's own controls sit over
/// one of its ends (macOS's traffic lights on the left, the caption buttons on the right on
/// Windows): [`title_area_css`] gives it their room.
const TITLE_AREA: &str = "position: absolute; left: 0px; top: 0px; right: 0px; display: flex; \
                          flex-direction: row; align-items: center; gap: 6px; -azul-app-region: drag;";
/// The title area's height without window controls taller than its buttons, px.
const TITLE_HEIGHT: f32 = 40.0;
/// The title area's own room at each end, beside the window controls' room, px.
const TITLE_PAD: f32 = 10.0;
/// A control in the drag region keeps its clicks.
const NO_DRAG: &str = "-azul-app-region: no-drag;";
/// A floating group of map controls (Apple Maps' rounded pills).
const CONTROL_GROUP: &str = "display: flex; flex-direction: column; background: \
                             system:window-background; border-radius: 8px; box-shadow: 0px 1px \
                             4px rgba(0,0,0,0.3); -azul-app-region: no-drag; @theme(flora) { \
                             border-radius: 5px; box-shadow: 0px 1px 4px rgba(48, 45, 38, 0.3); \
                             @media (prefers-color-scheme: dark) { box-shadow: 0px 1px 4px \
                             rgba(0, 0, 0, 0.55); } }";
/// The controls' column, bottom right.
const CONTROLS: &str = "position: absolute; right: 12px; bottom: 30px; display: flex; \
                        flex-direction: column; gap: 10px;";
/// The part of the window's left edge the open sidebar covers (its left
/// offset, its width, a gap): a framed route stays right of it.
const SIDEBAR_INSET: f32 = 310.0;
/// The floating sidebar, below the title area.
const SIDEBAR: &str = "position: absolute; left: 10px; top: 44px; bottom: 26px; width: 290px; \
                       display: flex; flex-direction: column; gap: 12px; padding: 12px; \
                       background: system:window-background; color: system:text; \
                       border-radius: 12px; box-shadow: 0px 2px 10px rgba(0,0,0,0.25); \
                       @theme(flora) { border-radius: 5px; box-shadow: 0px 2px 10px rgba(48, \
                       45, 38, 0.22); @media (prefers-color-scheme: dark) { box-shadow: 0px 2px \
                       10px rgba(0, 0, 0, 0.5); } }";
const COLUMN: &str = "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;";
const TRAVEL_PANEL: &str = "display: flex; flex-direction: column; gap: 6px;";
const ROW: &str = "display: flex; flex-direction: row; align-items: center; gap: 6px;";
const FIELDS: &str =
    "display: flex; flex-direction: column; gap: 6px; flex-grow: 1; min-width: 0px;";
const FIELD_GROW: &str = "flex-grow: 1; min-width: 0px;";
const FROM_MARK: &str = "width: 10px; height: 10px; border-radius: 6px; border: 2px solid \
                         #1a73e8; background: #ffffff; @theme(flora) { border: 2px solid \
                         system:accent; background: system:control-background; }";
const TO_MARK: &str = "width: 10px; height: 10px; border-radius: 6px; border: 2px solid \
                       #e5322d; background: #e5322d; @theme(flora) { border: 2px solid #7E4A42; \
                       background: #7E4A42; @media (prefers-color-scheme: dark) { border: 2px \
                       solid #B3837A; background: #B3837A; } }";
/// The typeahead's suggestions under the travel fields.
const SUGGESTIONS: &str = "display: flex; flex-direction: column; gap: 2px;";
const NO_SUGGESTIONS: &str = "font-size: 12px; color: system:secondary-text; padding: 4px 6px;";
/// A suggestion's row: the place, then its source, small.
const SUGGESTION_ROW: &str = "display: flex; flex-direction: row; align-items: center; gap: 4px;";
const SOURCE_TAG: &str = "font-size: 10px; color: system:secondary-text; white-space: nowrap;";
/// The search results' credits under the list.
const CREDITS: &str = "font-size: 10px; color: system:secondary-text; padding: 2px 6px; white-space: normal;";
const DISTANCE: &str = "flex-grow: 1; text-align: right; font-size: 13px; color: \
                        system:secondary-text; white-space: nowrap;";
/// The line saying where a route request goes while the routing server is asked.
const PRIVACY_NOTE: &str = "font-size: 11px; color: system:secondary-text; white-space: normal;";
/// The turn-by-turn steps: their own scrolling box, so the panel keeps its
/// controls in sight in a small window.
const STEPS: &str = "display: flex; flex-direction: column; align-items: stretch; gap: 2px; \
                     max-height: 220px; overflow-x: hidden; overflow-y: auto; min-height: 0px;";
/// A step's row: as wide as the list, its words wrapping inside it.
const STEP_ROW: &str = "min-width: 0px; height: auto; justify-content: flex-start; text-align: \
                        left; white-space: normal;";
/// The zoom a picked step is shown at, at least: its turn in sight.
const STEP_ZOOM: f32 = 15.0;
const RECENTS_SECTION: &str =
    "display: flex; flex-direction: column; gap: 4px; flex-grow: 1; min-height: 0px;";
const SECTION_HEAD: &str = "display: flex; flex-direction: row; align-items: center; \
                            font-size: 12px; font-weight: bold; color: system:secondary-text; \
                            @theme(flora) { font-size: 11px; text-transform: uppercase; \
                            letter-spacing: 0.12em; }";
const RECENTS_LIST: &str = "display: flex; flex-direction: column; gap: 2px; overflow-y: auto; \
                            flex-grow: 1; min-height: 0px;";
/// A place's card in its popover.
const CARD: &str = "display: flex; flex-direction: column; gap: 6px; padding: 4px; min-width: \
                    210px;";
const CARD_TITLE: &str = "font-size: 15px; font-weight: bold;";
const CARD_SUB: &str = "font-size: 12px; color: system:secondary-text;";
/// The tiles' licence line - required (ODbL, CC BY), so it stays, small.
const ATTRIBUTION: &str = "position: absolute; right: 4px; bottom: 3px; font-size: 10px; color: \
                           #333333; background: rgba(255,255,255,0.7); padding: 1px 4px; \
                           border-radius: 3px; @theme(flora) { color: #262521; background: \
                           rgba(251, 250, 246, 0.8); @media (prefers-color-scheme: dark) { \
                           color: #E7E7E7; background: rgba(29, 29, 29, 0.8); } }";
/// A problem with the pins file, as a small toast.
const NOTICE: &str = "position: absolute; left: 50%; bottom: 30px; width: 360px; margin-left: \
                      -180px; text-align: center; font-size: 12px; color: #ffffff; background: \
                      rgba(176,0,32,0.9); padding: 6px 10px; border-radius: 6px; @theme(flora) \
                      { color: #F4F2EA; background: rgba(126, 74, 66, 0.94); border-radius: 3px; }";
const LOCATION_DOT: &str = "width: 16px; height: 16px; background: #4285f4; border-radius: 8px; \
                            box-shadow: 0px 0px 0px 3px rgba(66,133,244,0.35); @theme(flora) { \
                            background: system:accent; box-shadow: 0px 0px 0px 3px \
                            system:selection-background; }";
const COMPASS_BADGE: &str = "position: absolute; right: 12px; top: 50px; width: 44px; height: \
                             44px; border-radius: 22px; background: rgba(20,20,28,0.85); border: \
                             2px solid #6a7080; display: flex; align-items: center; \
                             justify-content: center; box-shadow: 0px 1px 4px rgba(0,0,0,0.4);";
const NEEDLE_N: &str = "flex-grow: 1; background: #e74c3c; border-radius: 4px 4px 0px 0px;";
const NEEDLE_S: &str = "flex-grow: 1; background: #cfd2d8; border-radius: 0px 0px 4px 4px;";
/// The picked turn-by-turn step's part of the road route: amber, so it stands
/// out of the accent-coloured line in every theme.
const STEP_LINE: ColorU = ColorU {
    r: 0xff,
    g: 0xa0,
    b: 0x00,
    a: 0xff,
};
/// The outline under the road route that keeps it readable on any tile.
const ROUTE_CASING: ColorU = ColorU {
    r: 0xff,
    g: 0xff,
    b: 0xff,
    a: 0xff,
};

// ==== State ====

/// A place picked for a travel end from the typeahead's suggestions.
#[derive(Clone, PartialEq)]
struct Picked {
    /// What the field shows for it (`Frankfurt am Main, Germany`): the end is
    /// this place while the field's text is still this label.
    label: String,
    at: (f64, f64),
}

/// The travel panel: two ends as typed, the places picked for them, and how
/// to go.
#[derive(Default)]
struct Travel {
    from: String,
    to: String,
    mode: TravelMode,
    from_pick: Option<Picked>,
    to_pick: Option<Picked>,
}

impl Travel {
    fn text(&self, end: End) -> &str {
        match end {
            End::From => &self.from,
            End::To => &self.to,
        }
    }

    fn text_mut(&mut self, end: End) -> &mut String {
        match end {
            End::From => &mut self.from,
            End::To => &mut self.to,
        }
    }

    fn pick_mut(&mut self, end: End) -> &mut Option<Picked> {
        match end {
            End::From => &mut self.from_pick,
            End::To => &mut self.to_pick,
        }
    }

    /// The place an end's text names: the picked place while the text is
    /// its label, else typed coordinates.
    fn place(&self, end: End) -> Option<(f64, f64)> {
        let pick = match end {
            End::From => &self.from_pick,
            End::To => &self.to_pick,
        };
        match pick {
            Some(p) if p.label == self.text(end) => Some(p.at),
            _ => parse_place(self.text(end)),
        }
    }
}

/// What the typeahead shows under the travel fields.
struct Suggestions {
    /// The field the suggestions are for.
    end: End,
    /// The text the search was for.
    query: String,
    list: Vec<Suggestion>,
    /// The row Return picks; Up and Down move it.
    highlighted: usize,
    /// The online search could not be asked: the list says it is offline.
    offline: bool,
}

struct MapState {
    viewport: MapViewport,
    layer: MapTileLayer,
    locating: bool,
    last_fix: Option<(f64, f64)>,
    /// The recent places (dropped pins), oldest first as the pins file
    /// keeps them; the sidebar shows them newest first.
    pins: Vec<(f64, f64)>,
    /// The place whose card is open (an index into `pins`).
    selected: Option<usize>,
    sidebar_open: bool,
    travel: Travel,
    mag_x: f32,
    mag_y: f32,
    has_mag: bool,
    locate_failed: bool,
    locate_ticks: u32,
    /// The viewport changed and is not kept yet; ticks since the change.
    view_dirty: bool,
    view_idle_ticks: u32,
    /// azul-appkit's kit and the data root its file jobs run against.
    kit: RefAny,
    data_root: PathBuf,
    /// The pins file's key in the data tree (`maps/pins.json`).
    pins_key: String,
    /// A pins write is in flight; another change waits for it (one writer).
    saving: bool,
    save_pending: bool,
    /// What went wrong with the pins file, shown as a toast.
    notice: String,
    /// The window's size as the last build saw it - the map fills the window,
    /// so this is the map's: where a place is on screen between builds.
    size: Cell<(f32, f32)>,
    /// The travel panel's route, as the route worker answered the newest
    /// request (`route.rs`).
    route: Option<RouteReply>,
    /// How far the route worker of the newest request got, until it answers.
    route_progress: Option<RouteProgress>,
    /// The routing tiles (`--routing`), shared by every route worker ...
    routing: Option<Tiles>,
    /// ... else the routing server (`--routing-online`); with neither a route
    /// is the straight-line estimate.
    online: Option<Online>,
    /// The turn-by-turn step picked in the route panel: the map is centred on
    /// it and its part of the line is drawn over the rest.
    selected_step: Option<usize>,
    /// What the newest route request asked: the ends and the mode.
    route_asked: Option<((f64, f64), (f64, f64), TravelMode)>,
    /// The newest route request's number, shared with the route workers: an
    /// older one gives up, its reply is dropped.
    route_latest: Arc<AtomicU64>,
    /// The travel field typed in last and when, while its text waits for the
    /// typeahead: the timer tick searches once the typing rested
    /// [`TYPEAHEAD_REST_MS`].
    typed: Option<(End, std::time::Instant)>,
    /// The typeahead's suggestions, as the geocoder worker answered the
    /// newest search; `None` hides the list.
    suggestions: Option<Suggestions>,
    /// The field whose first suggestion is picked as soon as it comes back:
    /// Return was pressed before the search answered.
    pick_first: Option<End>,
    /// The newest search's number, shared with the geocoder workers (as
    /// `route_latest`).
    search_latest: Arc<AtomicU64>,
    /// The online services of the session and the cache of their answers (`services.rs`).
    services: crate::services::Services,
    /// The public transport panel: its time and its journeys (`transit_panel.rs`).
    transit: transit_panel::TransitState,
    /// The navigation under way and the position source (`drive_ui.rs`).
    drive: drive_ui::DriveState,
    /// The saved lists, the recent searches, imports and their review (`lists_ui.rs`).
    lists: lists_ui::ListsState,
    /// The offline areas: the kept ones and the download under way (`offline_ui.rs`).
    offline_areas: offline_ui::OfflineState,
}

impl MapState {
    /// Where the travel panel's route comes from, for its mode.
    fn route_source(&self) -> RouteSource {
        route_source(self.routing.is_some(), self.online.is_some(), self.travel.mode)
    }

    fn heading(&self) -> Option<f32> {
        if !self.has_mag {
            return None;
        }
        Some((self.mag_y.atan2(self.mag_x).to_degrees() + 360.0) % 360.0)
    }

    /// The viewport moved: announced, and kept once it rests.
    fn moved(&mut self) {
        println!(
            "{}",
            view_line(
                self.viewport.centre_lat_deg,
                self.viewport.centre_lon_deg,
                self.viewport.zoom
            )
        );
        self.view_dirty = true;
        self.view_idle_ticks = 0;
    }

    fn zoom_by(&mut self, delta: f32) {
        let min = MIN_ZOOM.max(self.layer.min_zoom as f32);
        let max = MAX_ZOOM.min(self.layer.max_zoom as f32);
        self.viewport.zoom = (self.viewport.zoom + delta).clamp(min, max);
        self.moved();
    }

    fn centre_on(&mut self, lat: f64, lon: f64) {
        self.viewport.centre_lat_deg = lat;
        self.viewport.centre_lon_deg = lon;
        self.moved();
    }

    fn toggle_locate(&mut self) {
        self.locating = !self.locating;
        if self.locating {
            self.locate_failed = false;
            self.locate_ticks = 0;
        }
    }

    fn pan(&mut self, dx: f64, dy: f64) {
        let z_int = self.viewport.zoom.floor() as i32;
        let tile_count = (1u32 << z_int.max(0) as u32) as f64;
        let (lon, lat) = pan_tiles(
            self.viewport.centre_lon_deg,
            self.viewport.centre_lat_deg,
            tile_count,
            dx / 2.0,
            dy / 2.0,
        );
        self.centre_on(lat, lon);
    }

    /// The travel panel's ends as places: the start as typed, or - left
    /// empty - where you are once that is known; the destination as typed.
    fn travel_ends(&self) -> (Option<(f64, f64)>, Option<(f64, f64)>) {
        let from = if self.travel.from.trim().is_empty() {
            self.last_fix
        } else {
            self.travel.place(End::From)
        };
        (from, self.travel.place(End::To))
    }

    /// The typing in the travel field `end` rested. Its text is a search for
    /// the geocoder worker - unless it is empty or already names a place
    /// (typed coordinates, a picked label): then the end changed, and the
    /// window shows it (`true`).
    fn typeahead(&mut self, end: End) -> (Option<SearchRequest>, bool) {
        let query = self.travel.text(end).trim().to_string();
        // Whatever is still in flight answers for older text.
        let id = self.search_latest.fetch_add(1, Ordering::AcqRel) + 1;
        if query.is_empty() || self.travel.place(end).is_some() {
            self.suggestions = None;
            self.announce_travel();
            // Typed coordinates are a place like a picked one: with both ends
            // places the map frames the way between them.
            if self.travel.place(end).is_some() {
                if let (Some(a), Some(b)) = self.travel_ends() {
                    self.frame_travel(a, b);
                }
            }
            return (None, true);
        }
        let near = (self.viewport.centre_lat_deg, self.viewport.centre_lon_deg);
        let request = SearchRequest {
            id,
            end,
            query,
            near: Some(near),
            online: self.services.transit.clone(),
            language: azul_appkit::l10n::locale().chars().take(2).collect(),
            saved: Arc::clone(&self.lists.lists),
            latest: self.search_latest.clone(),
            on_done: on_search_done,
        };
        (Some(request), false)
    }

    /// Picks the typeahead's suggestion `index`: its field shows the place's
    /// label, the end is that place, the list closes. Once both ends are
    /// places the map frames the way between them, else it shows the place.
    fn pick(&mut self, index: usize) -> bool {
        let Some(list) = self.suggestions.take() else {
            return false;
        };
        let Some(place) = list.list.get(index) else {
            return false;
        };
        let label = place.label(azul_appkit::l10n::locale().starts_with("de"));
        *self.travel.text_mut(list.end) = label.clone();
        // The search is remembered, and a destination among the recent ones.
        lists_ui::remember_search(self, &list.query);
        if list.end == End::To {
            lists_ui::remember_destination(self, &label, (place.lat, place.lon));
        }
        *self.travel.pick_mut(list.end) = Some(Picked {
            label,
            at: (place.lat, place.lon),
        });
        self.typed = None;
        self.pick_first = None;
        self.announce_travel();
        match self.travel_ends() {
            (Some(a), Some(b)) => self.frame_travel(a, b),
            _ => self.show_at(place.lat, place.lon, PICKED_PLACE_ZOOM),
        }
        true
    }

    /// Frames the way from `a` to `b` in the part of the window the sidebar
    /// leaves free.
    fn frame_travel(&mut self, a: (f64, f64), b: (f64, f64)) {
        let inset = if self.sidebar_open { SIDEBAR_INSET } else { 0.0 };
        let (lat, lon, zoom) = fit_view(a, b, self.size.get(), inset);
        self.show_at(lat, lon, zoom);
    }

    /// Centres the map on `lat`, `lon` at `zoom`, within the layer's zooms.
    fn show_at(&mut self, lat: f64, lon: f64, zoom: f32) {
        let min = MIN_ZOOM.max(self.layer.min_zoom as f32);
        let max = MAX_ZOOM.min(self.layer.max_zoom as f32);
        self.viewport.zoom = zoom.clamp(min, max);
        self.centre_on(lat, lon);
    }

    /// The travel panel changed: announced for scripts.
    fn announce_travel(&self) {
        let (from, to) = self.travel_ends();
        println!("{}", travel_line(self.travel.mode, from, to));
    }

    /// The route request for the travel panel as it is now - `None` when it
    /// asks for the route already asked for, or an end is not a place. A new
    /// request supersedes the one in flight (`route.rs`).
    fn route_request(&mut self) -> Option<RouteRequest> {
        let (Some(from), Some(to)) = self.travel_ends() else {
            if self.route_asked.take().is_some() {
                // Supersede what is still in flight: no answer for ends that
                // are gone.
                self.route_latest.fetch_add(1, Ordering::AcqRel);
            }
            self.route = None;
            return None;
        };
        let asked = (from, to, self.travel.mode);
        if self.route_asked == Some(asked) {
            return None;
        }
        self.route_asked = Some(asked);
        self.route = None;
        self.route_progress = None;
        self.selected_step = None;
        let id = self.route_latest.fetch_add(1, Ordering::AcqRel) + 1;
        Some(RouteRequest {
            id,
            from,
            to,
            mode: self.travel.mode,
            tiles: self.routing.clone(),
            online: self.online.clone(),
            language: azul_appkit::l10n::locale().chars().take(2).collect(),
            latest: self.route_latest.clone(),
            on_done: on_route_done,
            on_progress: on_route_progress,
        })
    }
}

/// `(lat, lon)` as the map widget takes it.
const fn at((lat_deg, lon_deg): (f64, f64)) -> MapLatLon {
    MapLatLon { lat_deg, lon_deg }
}

/// The travel panel may have changed: its route is asked of a route worker
/// (`MapState::route_request`), never worked out here, on the UI thread.
fn ask_route(app: &RefAny, info: &mut CallbackInfo) {
    let mut state = app.clone();
    let request = state
        .downcast_mut::<MapState>()
        .and_then(|mut s| s.route_request());
    if let Some(request) = request {
        info.add_thread(
            ThreadId::unique(),
            Thread::create(RefAny::new(request), app.clone(), route::route_worker),
        );
    }
    // Public transport is a timetable question: its journeys are asked of their own worker.
    transit_panel::ask(app, info);
}

/// A route came back from the route worker: shown when it answers the newest
/// request, dropped when the panel has asked for another since.
extern "C" fn on_route_done(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(reply) = msg.downcast_ref::<RouteReply>().map(|r| r.clone()) else {
        return Update::DoNothing;
    };
    {
        let Some(mut s) = app.downcast_mut::<MapState>() else {
            return Update::DoNothing;
        };
        if s.route_latest.load(Ordering::Acquire) != reply.id {
            return Update::DoNothing;
        }
        show_route(&mut s, reply);
    }
    // `--navigate`: the road route is there, the navigation starts.
    drive_ui::maybe_autostart(&app, &mut info);
    Update::RefreshDom
}

/// The newest route arrived: announced for scripts, shown on the panel and the map.
fn show_route(s: &mut MapState, reply: RouteReply) {
    println!("{}", route_reply_line(reply.mode, reply.route, reply.compute_ms));
    match &reply.kind {
        RouteKind::Road => println!(
            "AZMAPS_ROAD {} {:.1} {:.0} {} {} {} {} {}",
            reply.mode.key(),
            reply.route.km,
            reply.route.minutes,
            reply.road.len(),
            reply.tiles,
            reply.attempts,
            reply.source.key(),
            reply.steps.len()
        ),
        RouteKind::NoRoute(why) => println!("AZMAPS_NO_ROUTE {} {}", reply.mode.key(), why.key()),
        RouteKind::Estimate => {}
    }
    if reply.kind == RouteKind::Road && reply.source == RouteSource::Online {
        println!(
            "AZMAPS_ROUTE_ORIGIN {} {}",
            reply.mode.key(),
            if reply.offline { "offline" } else { "online" }
        );
    }
    s.route_progress = None;
    s.route = Some(reply);
}

/// The route worker got further (a tile arrived, the search began): the
/// panel says how far, while the request is the newest.
extern "C" fn on_route_progress(mut app: RefAny, mut msg: RefAny, _info: CallbackInfo) -> Update {
    let Some(step) = msg.downcast_ref::<RouteProgress>().map(|p| *p) else {
        return Update::DoNothing;
    };
    let Some(mut s) = app.downcast_mut::<MapState>() else {
        return Update::DoNothing;
    };
    if s.route_latest.load(Ordering::Acquire) != step.id || s.route.is_some() {
        return Update::DoNothing;
    }
    let p = step.progress;
    println!(
        "AZMAPS_ROUTE_PROGRESS {} {} {} {}",
        p.attempt,
        match p.stage {
            azul_routing::Stage::Fetching => "fetching",
            azul_routing::Stage::Searching => "searching",
        },
        p.tiles_done,
        p.tiles_wanted
    );
    s.route_progress = Some(step);
    Update::RefreshDom
}

// ==== Layout ====

/// `37.7749° N 122.4194° W`.
fn coords(lat: f64, lon: f64) -> String {
    format!(
        "{:.4}\u{b0} {} {:.4}\u{b0} {}",
        lat.abs(),
        if lat >= 0.0 { "N" } else { "S" },
        lon.abs(),
        if lon >= 0.0 { "E" } else { "W" }
    )
}

/// An icon button: its icon, its accessible name, its id, its action.
fn tool(app: &RefAny, icon: &str, name: &str, id: AzString, on_click: ButtonOnClickCallbackType) -> Dom {
    Button::create("")
        .with_icon(icon)
        .with_on_click(app.clone(), on_click)
        .dom()
        .with_id(id)
        .with_accessibility_name(label(name))
}

/// The title area over the map: the window moves by it; the sidebar's
/// button on the left, the settings on the right, no text.
/// The title area's CSS, clear of the window's own controls where they sit over it.
fn title_area_css(insets: kit::TitleInsets) -> String {
    format!(
        "{TITLE_AREA} height: {:.0}px; padding: 0px {:.0}px 0px {:.0}px;",
        insets.height.max(TITLE_HEIGHT),
        insets.right + TITLE_PAD,
        insets.left + TITLE_PAD
    )
}

/// The title area over the map: the sidebar's toggle at its start, Settings at its end, the
/// window controls' room `insets` beside them.
fn title_area(s: &MapState, app: &RefAny, insets: kit::TitleInsets) -> Dom {
    let toggle_name = label(if s.sidebar_open { "azmaps-hide-sidebar" } else { "azmaps-show-sidebar" });
    Dom::create_div()
        .with_css(title_area_css(insets).as_str())
        .with_id(ids::TITLE)
        .with_child(
            Dom::create_div().with_css(NO_DRAG).with_child(
                Button::create("")
                    .with_icon("view_sidebar")
                    .with_toggled(s.sidebar_open)
                    .with_on_click(app.clone(), on_toggle_sidebar as ButtonOnClickCallbackType)
                    .dom()
                    .with_id(ids::SIDEBAR_TOGGLE)
                    .with_accessibility_name(toggle_name),
            ),
        )
        .with_child(Dom::create_div().with_css("flex-grow: 1; height: 100%;"))
        .with_child(
            Dom::create_div()
                .with_css(NO_DRAG)
                .with_child(tool(app, "settings", "azmaps-settings", ids::SETTINGS, on_settings_open)),
        )
}

/// One travel field: which one, and the app.
struct FieldRef {
    app: RefAny,
    end: End,
}

/// One travel mode's button: which mode, and the app.
struct ModeRef {
    app: RefAny,
    mode: TravelMode,
}

/// The travel panel (Google-Maps-like): start and destination with a swap
/// button, the modes, the distance once both ends are places.
fn travel_panel(s: &MapState, app: &RefAny) -> Dom {
    let field = |mark: &str, text: &str, placeholder: &str, name: &str, id: AzString, end: End| {
        Dom::create_div()
            .with_css(ROW)
            .with_child(Dom::create_div().with_css(mark))
            .with_child(
                Dom::create_div().with_css(FIELD_GROW).with_child(
                    TextInput::create()
                        .with_text(text)
                        .with_placeholder(label(placeholder))
                        .with_accessibility_name(label(name))
                        .with_on_text_input(
                            RefAny::new(FieldRef {
                                app: app.clone(),
                                end,
                            }),
                            on_travel_text as TextInputOnTextInputCallbackType,
                        )
                        .with_on_virtual_key_down(
                            RefAny::new(FieldRef {
                                app: app.clone(),
                                end,
                            }),
                            on_travel_key as TextInputOnVirtualKeyDownCallbackType,
                        )
                        .dom()
                        .with_id(id),
                ),
            )
    };
    let from_placeholder = if s.last_fix.is_some() { "azmaps-my-location" } else { "azmaps-from" };
    let fields = Dom::create_div()
        .with_css(FIELDS)
        .with_child(field(
            FROM_MARK,
            &s.travel.from,
            from_placeholder,
            "azmaps-start",
            ids::TRAVEL_FROM,
            End::From,
        ))
        .with_child(field(
            TO_MARK,
            &s.travel.to,
            "azmaps-to",
            "azmaps-destination",
            ids::TRAVEL_TO,
            End::To,
        ));
    let ends = Dom::create_div()
        .with_css(ROW)
        .with_child(fields)
        .with_child(tool(app, "swap_vert", "azmaps-swap", ids::TRAVEL_SWAP, on_travel_swap))
        .with_child(tool(app, "mic", "azmaps-speak-destination", ids::TRAVEL_SPEAK, on_speak_destination));

    let mut modes = Dom::create_div().with_css(ROW);
    for mode in TravelMode::ALL {
        modes.add_child(
            Button::create("")
                .with_icon(mode.icon())
                .with_toggled(s.travel.mode == mode)
                .with_on_click(
                    RefAny::new(ModeRef {
                        app: app.clone(),
                        mode,
                    }),
                    on_travel_mode as ButtonOnClickCallbackType,
                )
                .dom()
                .with_id(ids::travel_mode(mode.key()))
                .with_accessibility_name(label(mode.label())),
        );
    }
    if let (Some(a), Some(b)) = s.travel_ends() {
        // The distance as the crow flies at once; the route worker's time
        // once it answers for these ends.
        let mut text = distance_text(distance_km(a, b));
        let reply = s
            .route
            .as_ref()
            .filter(|r| r.from == a && r.to == b && r.mode == s.travel.mode);
        match reply.map(|r| &r.kind) {
            // Along the roads: the road's length, not the crow's.
            Some(RouteKind::Road) => {
                if let Some(r) = reply {
                    text = distance_text(r.route.km);
                    text.push_str(" \u{b7} ");
                    text.push_str(&duration_text(r.route.minutes));
                    if r.offline {
                        text.push_str(" \u{b7} ");
                        text.push_str(label("azmaps-offline").as_str());
                    }
                }
            }
            Some(RouteKind::Estimate) => {
                if let Some(r) = reply {
                    text.push_str(" \u{b7} ");
                    text.push_str(&duration_text(r.route.minutes));
                    text.push_str(" \u{b7} ");
                    text.push_str(label("azmaps-route-estimate").as_str());
                }
            }
            Some(RouteKind::NoRoute(why)) => {
                text.push_str(" \u{b7} ");
                text.push_str(label(why.label()).as_str());
            }
            None if s.route_source() == RouteSource::Online => {
                text.push_str(" \u{b7} ");
                text.push_str(label("azmaps-route-asking").as_str());
            }
            None => {
                if let Some(step) = s.route_progress {
                    text.push_str(" \u{b7} ");
                    text.push_str(&t_args(
                        "azmaps-route-searching",
                        &[
                            ("done", Arg::from(step.progress.tiles_done.to_string())),
                            ("total", Arg::from(step.progress.tiles_wanted.to_string())),
                        ],
                    ));
                }
            }
        }
        modes.add_child(
            Dom::create_div()
                .with_css(DISTANCE)
                .with_id(ids::TRAVEL_DISTANCE)
                .with_child(Dom::create_span_with_text(text)),
        );
    }

    let mut panel = Dom::create_div()
        .with_css(TRAVEL_PANEL)
        .with_id(ids::TRAVEL)
        .with_accessibility_name(label("azmaps-directions"))
        .with_child(ends);
    if let Some(list) = &s.suggestions {
        panel.add_child(suggestion_list(list, app));
    }
    panel.add_child(modes);
    if let (Some(_), Some(_)) = s.travel_ends() {
        if s.route_source() == RouteSource::Online {
            panel.add_child(
                Dom::create_div()
                    .with_css(PRIVACY_NOTE)
                    .with_id(ids::ROUTE_PRIVACY)
                    .with_child(Dom::create_span_with_text(label("azmaps-route-privacy"))),
            );
        }
    }
    if s.travel.mode == TravelMode::Transit {
        panel.add_child(transit_panel::panel(s, app));
    } else if let Some(start) = drive_ui::start_row(s, app) {
        panel.add_child(start);
    }
    if let Some(reply) = s
        .route
        .as_ref()
        .filter(|r| r.kind == RouteKind::Road && !r.steps.is_empty())
    {
        panel.add_child(step_list(reply, s.selected_step, app));
    }
    panel
}

/// One step's row: which one, and the app.
struct StepRef {
    app: RefAny,
    index: usize,
}

/// The routing server's turn-by-turn steps under the route's summary: each
/// row its arrow, what to do and how far it goes; the picked one marked. The
/// list scrolls in its own box, so a long trip leaves the panel usable in a
/// small window.
fn step_list(reply: &RouteReply, selected: Option<usize>, app: &RefAny) -> Dom {
    let mut rows = Dom::create_div()
        .with_css(STEPS)
        .with_id(ids::ROUTE_STEPS)
        .with_accessibility_name(label("azmaps-route-steps"));
    for (index, step) in reply.steps.iter().enumerate() {
        let mut text = step.instruction.clone();
        if step.distance_m >= 1.0 {
            text.push_str(" \u{b7} ");
            text.push_str(&distance_text(step.distance_m / 1000.0));
        }
        rows.add_child(
            Button::create(text)
                .with_icon(step.kind.icon())
                .with_toggled(selected == Some(index))
                .with_on_click(
                    RefAny::new(StepRef {
                        app: app.clone(),
                        index,
                    }),
                    on_step as ButtonOnClickCallbackType,
                )
                .dom()
                .with_id(ids::route_step(index))
                .with_css(STEP_ROW)
                // The row's own width, not its parts': the label shrinks and wraps.
                .with_css_property(CssPropertyWithConditions::simple(CssProperty::width(
                    LayoutWidth::px(PixelValue::percent(100.0)),
                ))),
        );
    }
    rows
}

/// A step was picked: the map centres on where it starts and its part of the
/// line is drawn over the rest.
extern "C" fn on_step(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some((mut app, index)) = data.downcast_ref::<StepRef>().map(|r| (r.app.clone(), r.index)) else {
        return Update::DoNothing;
    };
    let Some(mut s) = app.downcast_mut::<MapState>() else {
        return Update::DoNothing;
    };
    let Some(first) = s
        .route
        .as_ref()
        .and_then(|r| r.steps.get(index).and_then(|step| r.road.get(step.first_point)))
        .copied()
    else {
        return Update::DoNothing;
    };
    s.selected_step = Some(index);
    s.viewport.zoom = s.viewport.zoom.max(STEP_ZOOM);
    s.centre_on(first.0, first.1);
    println!("AZMAPS_STEP {index} {:.4},{:.4}", first.0, first.1);
    Update::RefreshDom
}

/// One suggestion's row: which one, and the app.
struct SuggestionRef {
    app: RefAny,
    index: usize,
}

/// The typeahead's suggestions under the travel fields: a row per place, the
/// one Return picks highlighted, or a line saying no place is known by that
/// name.
fn suggestion_list(list: &Suggestions, app: &RefAny) -> Dom {
    let german = azul_appkit::l10n::locale().starts_with("de");
    let mut rows = Dom::create_div()
        .with_css(SUGGESTIONS)
        .with_id(ids::SUGGESTIONS)
        .with_accessibility_name(label("azmaps-suggestions"));
    if list.list.is_empty() {
        let none = t_args("azmaps-no-places", &[("query", Arg::from(list.query.clone()))]);
        return rows.with_child(
            Dom::create_div()
                .with_css(NO_SUGGESTIONS)
                .with_child(Dom::create_span_with_text(none)),
        );
    }
    for (index, place) in list.list.iter().enumerate() {
        rows.add_child(
            Dom::create_div()
                .with_css(SUGGESTION_ROW)
                .with_child(
                    Dom::create_div().with_css(FIELD_GROW).with_child(
                        Button::create(place.label(german))
                            .with_icon(place.kind.icon())
                            .with_toggled(index == list.highlighted)
                            .with_on_click(
                                RefAny::new(SuggestionRef {
                                    app: app.clone(),
                                    index,
                                }),
                                on_suggestion as ButtonOnClickCallbackType,
                            )
                            .dom()
                            .with_id(ids::indexed("suggestion", index)),
                    ),
                )
                .with_child(
                    Dom::create_div()
                        .with_css(SOURCE_TAG)
                        .with_child(Dom::create_span_with_text(source_tag(&place.source))),
                ),
        );
    }
    // The sources of what is listed, credited in full under the list.
    let mut credits: Vec<String> = Vec::new();
    for place in &list.list {
        let credit = geocode::source_credit(&place.source);
        if matches!(place.source, SuggestionSource::Saved(_)) || credits.contains(&credit) {
            continue;
        }
        credits.push(credit);
    }
    if list.offline {
        credits.push(label("azmaps-offline").as_str().to_string());
    }
    rows.add_child(
        Dom::create_div()
            .with_css(CREDITS)
            .with_id(ids::SUGGESTION_CREDITS)
            .with_child(Dom::create_span_with_text(credits.join(" \u{b7} "))),
    );
    rows
}

/// A suggestion's source as the row says it, short: `OSM`, `AzMaps`, the saved list's name.
fn source_tag(source: &SuggestionSource) -> String {
    match source {
        SuggestionSource::Places => "AzMaps".to_string(),
        SuggestionSource::Transitous => "OSM".to_string(),
        SuggestionSource::Saved(list) => list.clone(),
    }
}

/// One place of the recents / on the map: which place, and the app.
struct PinRef {
    app: RefAny,
    index: usize,
}

/// The recent places, newest first; a click centres the map on one and
/// opens its card. Nothing at all while there are none.
fn recents(s: &MapState, app: &RefAny) -> Option<Dom> {
    if s.pins.is_empty() {
        return None;
    }
    let mut list = Dom::create_div()
        .with_css(RECENTS_LIST)
        .with_id(ids::RECENTS)
        .with_accessibility_name(label("azmaps-recents"));
    for (index, &(lat, lon)) in s.pins.iter().enumerate().rev() {
        list.add_child(
            Button::create(coords(lat, lon))
                .with_icon("place")
                .with_toggled(s.selected == Some(index))
                .with_on_click(
                    RefAny::new(PinRef {
                        app: app.clone(),
                        index,
                    }),
                    on_place_row as ButtonOnClickCallbackType,
                )
                .dom()
                .with_id(ids::indexed("place", index)),
        );
    }
    Some(
        Dom::create_div()
            .with_css(RECENTS_SECTION)
            .with_child(
                Dom::create_div()
                    .with_css(SECTION_HEAD)
                    .with_child(
                        Dom::create_div()
                            .with_css("flex-grow: 1;")
                            .with_child(Dom::create_span_with_text(label("azmaps-recents"))),
                    )
                    .with_child(tool(
                        app,
                        "delete_sweep",
                        "azmaps-clear-recents",
                        ids::CLEAR_PINS,
                        on_clear_pins,
                    )),
            )
            .with_child(list),
    )
}

/// The sidebar: travel, then the recent places. Nothing else.
fn sidebar(s: &MapState, app: &RefAny) -> Dom {
    let mut side = Dom::create_div()
        .with_css(SIDEBAR)
        .with_id(ids::SIDEBAR)
        .with_accessibility_name(label("azmaps-sidebar"))
        .with_child(travel_panel(s, app));
    if let Some(saved) = lists_ui::saved_section(s, app) {
        side.add_child(saved);
    }
    if let Some(list) = recents(s, app) {
        side.add_child(list);
    }
    side
}

/// A place's card, in the popover at its pin: what it is, where it is,
/// directions to it, remove it.
fn place_card(app: &RefAny, index: usize, lat: f64, lon: f64, address: Option<&str>) -> Dom {
    let pin = || {
        RefAny::new(PinRef {
            app: app.clone(),
            index,
        })
    };
    Dom::create_div()
        .with_css(CARD)
        .with_child(
            Dom::create_div()
                .with_css(CARD_TITLE)
                .with_child(Dom::create_span_with_text(match address {
                    // The search's address of the place, else what it is.
                    Some(address) => AzString::from(address.to_string()),
                    None => label("azmaps-dropped-pin"),
                })),
        )
        .with_child(
            Dom::create_div()
                .with_css(CARD_SUB)
                .with_child(Dom::create_span_with_text(coords(lat, lon))),
        )
        .with_child(
            Dom::create_div()
                .with_css(ROW)
                .with_child(
                    Button::with_type(label("azmaps-directions"), ButtonType::Primary)
                        .with_icon("directions")
                        .with_on_click(pin(), on_place_directions as ButtonOnClickCallbackType)
                        .dom()
                        .with_id(ids::indexed("place-directions", index)),
                )
                .with_child(
                    Button::create("")
                        .with_icon("star")
                        .with_on_click(pin(), on_place_favourite as ButtonOnClickCallbackType)
                        .dom()
                        .with_id(ids::indexed("place-favourite", index))
                        .with_accessibility_name(label("azmaps-favourite")),
                )
                .with_child(
                    Button::create("")
                        .with_icon("delete")
                        .with_on_click(pin(), on_place_remove as ButtonOnClickCallbackType)
                        .dom()
                        .with_id(ids::indexed("place-remove", index))
                        .with_accessibility_name(label("azmaps-remove")),
                ),
        )
}

/// A pin's icon: the red head with its white dot, its point on the place.
fn pin_icon(index: usize, (lat, lon): (f64, f64)) -> Dom {
    Dom::create_div()
        .with_css(PIN_HEAD)
        .with_child(Dom::create_div().with_css(PIN_DOT))
        .with_id(ids::indexed("place-pin", index))
        .with_accessibility_name(coords(lat, lon))
}

/// The travel line through `points`, in the theme's accent: the road route
/// (`road`) with an outline under it, else the straight preview, thinner. The
/// accent is a colour token, so the map resolves it against the theme it is
/// drawn in and the line follows a theme switch without a new colour here.
fn travel_polyline(points: MapLatLonVec, road: bool, mode: TravelMode) -> MapPolyline {
    let accent = SystemColorRef::Accent.to_color_token();
    if road && mode == TravelMode::Walk {
        // A walk is dots, as on every map: wider than a drive's line so each dot reads.
        MapPolyline::create(points)
            .with_color(accent)
            .with_width(6.0)
            .with_casing(ROUTE_CASING, 9.0)
            .with_dash(MapLineDash::Dotted)
    } else if road {
        MapPolyline::create(points)
            .with_color(accent)
            .with_width(5.0)
            .with_casing(ROUTE_CASING, 8.0)
    } else {
        MapPolyline::create(points).with_color(accent).with_width(3.0)
    }
}

/// What AzMaps puts on the map ([`plan_overlays`]) as the map widget's
/// overlays: the travel line (the road route, or the straight preview until
/// it arrives), its ends, the pins - the selected one with its card above it -
/// and where you are, on top.
fn map_overlays(s: &MapState, app: &RefAny) -> Vec<MapOverlay> {
    let travel = match s.travel_ends() {
        (Some(a), Some(b)) => Some((a, b)),
        _ => None,
    };
    let road = s.route.as_ref().and_then(|reply| {
        let current = travel == Some((reply.from, reply.to)) && reply.mode == s.travel.mode;
        current.then_some(reply.road.as_slice())
    });
    let here = if s.locating && s.drive.nav.is_none() { s.last_fix } else { None };
    let plan = plan_overlays(&s.pins, s.selected, travel, road, here);
    let mut out = Vec::new();
    // The picked journey leg by leg, the open saved list's places.
    out.extend(transit_panel::overlays(s));
    out.extend(lists_ui::markers(s));
    // While navigating the navigation draws its own route (the new one after a reroute).
    let line = if s.drive.nav.is_some() || s.travel.mode == TravelMode::Transit { None } else { plan.line };
    if let Some(line) = line {
        let points = MapLatLonVec::from_vec(line.points.iter().copied().map(at).collect());
        out.push(MapOverlay::Polyline(travel_polyline(points, line.road, s.travel.mode)));
        // The picked turn-by-turn step's part, over the rest of the line.
        let picked = s.selected_step.zip(s.route.as_ref()).and_then(|(index, reply)| {
            let step = reply.steps.get(index)?;
            let part = reply.road.get(step.first_point..=step.last_point.max(step.first_point + 1))?;
            Some(part.to_vec())
        });
        if let (true, Some(part)) = (line.road, picked) {
            let points = MapLatLonVec::from_vec(part.into_iter().map(at).collect());
            out.push(MapOverlay::Polyline(
                MapPolyline::create(points)
                    .with_color(STEP_LINE)
                    .with_width(7.0)
                    .with_casing(ROUTE_CASING, 10.0),
            ));
        }
    }
    let end = |id: u64, place: (f64, f64), css: &'static str, name: &str| {
        MapOverlay::Marker(
            MapMarker::create(id, at(place), Dom::create_div().with_css(css))
                .with_anchor(7.0, 7.0)
                .with_name(label(name)),
        )
    };
    if let Some(place) = plan.from {
        out.push(end(FROM_MARKER, place, FROM_MARK, "azmaps-start"));
    }
    if let Some(place) = plan.to {
        out.push(end(TO_MARKER, place, TO_MARK, "azmaps-destination"));
    }
    for pin in plan.pins {
        let (lat, lon) = pin.at;
        let mut marker = MapMarker::create(pin.index as u64, at(pin.at), pin_icon(pin.index, pin.at))
            .with_anchor(PIN_HALF_WIDTH, PIN_TIP_DEPTH)
            .with_name(AzString::from(coords(lat, lon)));
        if pin.open {
            let address = lists_ui::address_of(s, (lat, lon));
            marker = marker.with_popup(place_card(app, pin.index, lat, lon, address), true);
        }
        out.push(MapOverlay::Marker(marker));
    }
    if let Some(place) = plan.here {
        out.push(MapOverlay::Marker(
            MapMarker::create(HERE_MARKER, at(place), Dom::create_div().with_css(LOCATION_DOT))
                .with_anchor(8.0, 8.0)
                .with_name(label("azmaps-you-are-here")),
        ));
    }
    out.extend(drive_ui::overlays(s));
    out
}

/// The world at zoom 0 and 1, as OpenFreeMap serves it for the widget's own
/// layer (`scripts/fetch_map_seed_tiles.py` fetches them; data (c)
/// OpenStreetMap contributors, ODbL, in the OpenMapTiles schema, CC BY 4.0 -
/// the attribution line over the map credits them, `assets/seed/ATTRIBUTION.txt`
/// says so). Shipped so the map shows the world before its first download and
/// on a first start without a network; one set serves the light and the dark
/// look, a vector tile carries no colours.
const SEED_TILES: [(u8, u32, u32, &[u8]); 5] = [
    (0, 0, 0, include_bytes!("../assets/seed/0-0-0.pbf")),
    (1, 0, 0, include_bytes!("../assets/seed/1-0-0.pbf")),
    (1, 1, 0, include_bytes!("../assets/seed/1-1-0.pbf")),
    (1, 0, 1, include_bytes!("../assets/seed/1-0-1.pbf")),
    (1, 1, 1, include_bytes!("../assets/seed/1-1-1.pbf")),
];

/// What the map's tile fetches run with: the tiles kept in the data folder
/// (`maps/tiles/`, the widget's default cap) so a pan back or a restart shows
/// them without a download, and the shipped world tiles when the map shows the
/// server they came from. Without a data folder the tiles stay in memory.
fn map_setup(data_root: &std::path::Path, layer: &MapTileLayer, setup: MapSetup) -> MapSetup {
    let mut setup = setup;
    if !data_root.as_os_str().is_empty() {
        let folder = data_root.join("maps").join("tiles");
        setup = setup.with_disk_cache(AzString::from(folder.to_string_lossy().into_owned()), 0);
    }
    if layer.url_template == MapTileLayer::default().url_template {
        for (z, x, y, bytes) in SEED_TILES {
            setup = setup.with_seed_tile(MapTileId { z, x, y }, U8Vec::from(bytes.to_vec()));
        }
    }
    setup
}

/// The map is in the tree: its fetches run with [`map_setup`].
extern "C" fn on_map_mount(mut data: RefAny, _: CallbackInfo, setup: MapSetup) -> MapSetup {
    let Some((root, layer)) = data
        .downcast_ref::<MapState>()
        .map(|s| (s.data_root.clone(), s.layer.clone()))
    else {
        return setup;
    };
    map_setup(&root, &layer, setup)
}

/// The map: the whole window - the tiles and AzMaps' overlays on them
/// ([`map_overlays`]), the compass. The map draws the overlays in its own
/// view and moves them with every pan, pinch, turn and tilt: a pan rebuilds
/// nothing here.
fn map_area(s: &MapState, app: &RefAny) -> Dom {
    let map = MapWidget::create(s.layer.clone())
        .with_viewport(s.viewport)
        .with_overlays(MapOverlayVec::from_vec(map_overlays(s, app)))
        .with_on_viewport_changed(app.clone(), on_viewport_changed)
        .with_on_pin_tap(app.clone(), on_pin_tap)
        .with_on_marker_tap(app.clone(), on_marker_tap)
        .with_on_mount(app.clone(), on_map_mount)
        .dom()
        .with_marker(OptionString::Some(ids::MAP_VIEW));
    let mut area = Dom::create_div()
        .with_css(MAP_AREA)
        .with_id(ids::MAP)
        .with_child(map);

    if s.locating {
        area.add_child(Dom::create_geolocation_probe(GeolocationProbeConfig {
            high_accuracy: true,
            background: false,
            max_accuracy_m: 0.0,
            min_interval_ms: 0,
        }));
    }

    if let Some(heading) = s.heading() {
        let needle = format!(
            "width: 6px; height: 30px; display: flex; flex-direction: column; transform: \
             rotate({:.1}deg);",
            -heading,
        );
        area.add_child(
            Dom::create_div()
                .with_css(COMPASS_BADGE)
                .with_accessibility_name(t_args(
                    "azmaps-heading",
                    &[
                        ("direction", Arg::word(&format!("azmaps-direction-{}", cardinal(heading).to_ascii_lowercase()), cardinal(heading))),
                        ("degrees", Arg::from(format!("{heading:03.0}"))),
                    ],
                ))
                .with_child(
                    Dom::create_div()
                        .with_css(needle.as_str())
                        .with_child(Dom::create_div().with_css(NEEDLE_N))
                        .with_child(Dom::create_div().with_css(NEEDLE_S)),
                ),
        );
    }
    area
}

/// The floating controls, bottom right: where am I, zoom.
fn map_controls(s: &MapState, app: &RefAny) -> Dom {
    let (locate_icon, locate_name) = if s.locate_failed {
        ("location_disabled", label("azmaps-location-unavailable"))
    } else {
        ("my_location", label("azmaps-show-location"))
    };
    Dom::create_div()
        .with_css(CONTROLS)
        .with_child(
            Dom::create_div().with_css(CONTROL_GROUP).with_child(
                Button::create("")
                    .with_icon(locate_icon)
                    .with_toggled(s.locating)
                    .with_on_click(app.clone(), on_locate as ButtonOnClickCallbackType)
                    .dom()
                    .with_id(ids::LOCATE)
                    .with_accessibility_name(locate_name),
            ),
        )
        .with_child(
            Dom::create_div()
                .with_css(CONTROL_GROUP)
                .with_child(tool(app, "add", "azmaps-zoom-in", ids::ZOOM_IN, on_zoom_in))
                .with_child(tool(app, "remove", "azmaps-zoom-out", ids::ZOOM_OUT, on_zoom_out)),
        )
}

/// The map screen: the map, and what floats on it.
fn screen(s: &MapState, app: &RefAny, dark: bool, insets: kit::TitleInsets) -> Dom {
    let mut root = Dom::create_div()
        .with_css(ROOT)
        .with_child(map_area(s, app))
        .with_child(title_area(s, app, insets));
    if s.sidebar_open {
        root.add_child(sidebar(s, app));
    }
    root.add_child(map_controls(s, app));
    // The routing server's credit beside the map's, while routes are asked of it.
    let mut credits = s.layer.attribution.as_str().to_string();
    if s.routing.is_none() && s.online.is_some() {
        credits.push_str(" \u{b7} ");
        credits.push_str(label("azmaps-routing-credit").as_str());
    }
    root.add_child(
        Dom::create_div()
            .with_css(ATTRIBUTION)
            .with_child(Dom::create_span_with_text(credits)),
    );
    if !s.notice.is_empty() {
        root.add_child(
            Dom::create_div()
                .with_css(NOTICE)
                .with_child(Dom::create_span_with_text(s.notice.as_str())),
        );
    }
    // The navigation's banner and bar; else the indicator of an external position source.
    let banner = drive_ui::banner(s, app, dark);
    if banner.is_empty() {
        if let Some(indicator) = drive_ui::indicator_on_map(s, None) {
            root.add_child(indicator);
        }
    }
    for part in banner {
        root.add_child(part);
    }
    root
}

extern "C" fn layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    let started = std::time::Instant::now();
    azul_appkit::l10n::begin_layout(&info);
    // Reading the mode makes a light / dark switch rebuild the window (the
    // tiles' style follows it).
    let dark = matches!(info.get_mode(), DarkLightMode::Dark);
    let size = (info.get_window_width(), info.get_window_height());
    let app = data.clone();
    let Some(s) = data.downcast_ref::<MapState>() else {
        return Dom::create_body();
    };
    s.size.set(size);
    let content = if kit::settings_open(&s.kit) {
        // azul-appkit's settings page: Appearance, Data, Shortcuts, About.
        Dom::create_div()
            .with_css(COLUMN)
            .with_child(kit::title_row(SPEC.name))
            .with_child(kit::settings_page(
                &s.kit,
                vec![
                    drive_ui::location_section(&s, &app, None),
                    drive_ui::routing_tiles_section(&s, &app),
                    offline_ui::section(&s, &app),
                    lists_ui::places_section(&s, &app),
                ],
            ))
    } else {
        screen(&s, &app, dark, kit::title_insets(&info))
    };
    let column = Dom::create_div().with_css(COLUMN).with_child(content);
    // The scope as the window's body: no UA margin, the full window height.
    let body = ShellThemeScope::create(column)
        .with_accent(ShellThemeAccent::Leaf)
        .body()
        .with_callback(EventFilter::Window(WindowEventFilter::VirtualKeyDown), app, on_key);
    if STATS.load(Ordering::Relaxed) {
        let n = LAYOUTS.fetch_add(1, Ordering::Relaxed) + 1;
        println!("AZMAPS_LAYOUT {n} {}", started.elapsed().as_micros());
    }
    body
}

// ==== Callbacks ====

/// Runs `f` on the map's state; the window is rebuilt afterwards.
fn with_map(data: &mut RefAny, f: impl FnOnce(&mut MapState)) -> Update {
    match data.downcast_mut::<MapState>() {
        Some(mut s) => {
            f(&mut *s);
            Update::RefreshDom
        }
        None => Update::DoNothing,
    }
}

/// The pins changed: announced, and written to the pins file on a Thread
/// (one write at a time; a change during a write is written after it).
fn save_pins(state_ref: &RefAny, info: &mut CallbackInfo) {
    let mut data = state_ref.clone();
    let job = {
        let Some(mut s) = data.downcast_mut::<MapState>() else {
            return;
        };
        println!("AZMAPS_PINS {}", s.pins.len());
        if s.saving {
            s.save_pending = true;
            return;
        }
        s.saving = true;
        s.save_pending = false;
        (
            s.data_root.clone(),
            FileJob::Put {
                key: s.pins_key.clone(),
                bytes: pins_to_json(&s.pins).into_bytes(),
            },
        )
    };
    kit::spawn_file_jobs(info, &job.0, vec![job.1], state_ref.clone(), TAG_SAVE, on_files_done);
}

extern "C" fn on_files_done(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(reply) = kit::take_reply(&mut msg) else {
        return Update::DoNothing;
    };
    let mut again = false;
    let update = with_map(&mut app, |s| match reply.tag {
        TAG_LOAD => {
            for outcome in reply.outcomes {
                match outcome {
                    FileOutcome::Got { result: Ok(Some(bytes)), .. } => {
                        match pins_from_json(&String::from_utf8_lossy(&bytes)) {
                            Ok(mut loaded) => {
                                // Pins dropped before the file arrived stay, after it.
                                loaded.append(&mut s.pins);
                                s.pins = loaded;
                                // Indexes moved: no card stays open on another place.
                                s.selected = None;
                                println!("AZMAPS_PINS_LOADED {}", s.pins.len());
                            }
                            Err(why) => {
                                println!("AZMAPS_PINS_ERROR {why}");
                                s.notice = t_args("azmaps-pins-unreadable", &[("why", Arg::from(why.to_string()))]);
                            }
                        }
                    }
                    FileOutcome::Got { result: Ok(None), .. } => println!("AZMAPS_PINS_LOADED 0"),
                    other => {
                        let why = other.error().unwrap_or_default();
                        println!("AZMAPS_PINS_ERROR {why}");
                        s.notice = t_args("azmaps-pins-unreadable", &[("why", Arg::from(why.to_string()))]);
                    }
                }
            }
        }
        _ => {
            s.saving = false;
            match reply.outcomes.iter().find_map(FileOutcome::error) {
                None => {
                    println!("AZMAPS_PINS_SAVED {}", s.pins.len());
                    s.notice.clear();
                }
                Some(why) => {
                    println!("AZMAPS_PINS_ERROR {why}");
                    s.notice = t_args("azmaps-pins-unsaved", &[("why", Arg::from(why.to_string()))]);
                }
            }
            again = s.save_pending;
        }
    });
    if again {
        save_pins(&app, &mut info);
    }
    update
}

/// The map was panned / zoomed / turned by the user. It moves its tiles and
/// AzMaps' overlays itself, in its own view: AzMaps only keeps the viewport
/// (announced, and saved once it rests). Nothing is rebuilt for a step of a
/// drag or a pinch.
extern "C" fn on_viewport_changed(mut data: RefAny, _info: CallbackInfo, vp: MapViewport) -> Update {
    let started = std::time::Instant::now();
    let Some(mut s) = data.downcast_mut::<MapState>() else {
        return Update::DoNothing;
    };
    s.viewport = vp;
    s.moved();
    if STATS.load(Ordering::Relaxed) {
        println!("AZMAPS_PAN map {}", started.elapsed().as_micros());
    }
    Update::DoNothing
}

/// A tap on the map drops a pin - unless a place's card is open: then the tap
/// closes it, as a tap outside a card does.
extern "C" fn on_pin_tap(mut data: RefAny, mut info: CallbackInfo, coord: MapLatLon) -> Update {
    let mut dropped = false;
    let update = with_map(&mut data, |s| {
        if s.selected.is_some() {
            s.selected = None;
            return;
        }
        s.pins.push((coord.lat_deg, coord.lon_deg));
        dropped = true;
    });
    if dropped {
        save_pins(&data, &mut info);
    }
    update
}

/// The pin of a `PinRef` callback: the app and the place's index.
fn pin_of(data: &mut RefAny) -> Option<(RefAny, usize)> {
    data.downcast_ref::<PinRef>().map(|p| (p.app.clone(), p.index))
}

/// A recent place's row: the map centres on it and its card opens.
extern "C" fn on_place_row(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, index)) = pin_of(&mut data) else {
        return Update::DoNothing;
    };
    let mut opened = None;
    let update = with_map(&mut app, |s| {
        if let Some((lat, lon)) = s.pins.get(index).copied() {
            s.centre_on(lat, lon);
            s.selected = Some(index);
            opened = Some((lat, lon));
            println!("AZMAPS_PLACE {index}");
        }
    });
    // The card shows the place's address once the search knows it.
    if let Some(at) = opened {
        lists_ui::ask_address(&app, &mut info, at);
    }
    update
}

/// A tap on a marker: a pin opens its card, or closes it when it is open;
/// the travel line's ends and where you are do nothing.
extern "C" fn on_marker_tap(mut data: RefAny, mut info: CallbackInfo, id: u64) -> Update {
    let Some(index) = pin_of_marker(id) else {
        return Update::DoNothing;
    };
    let mut opened = None;
    let update = with_map(&mut data, |s| {
        if s.selected == Some(index) {
            s.selected = None;
        } else if index < s.pins.len() {
            s.selected = Some(index);
            opened = s.pins.get(index).copied();
            println!("AZMAPS_PLACE {index}");
        }
    });
    if let Some(at) = opened {
        lists_ui::ask_address(&data, &mut info, at);
    }
    update
}

/// Directions to a place: it becomes the destination, the sidebar shows the
/// travel panel, the card closes; the route is asked of the route worker.
extern "C" fn on_place_directions(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, index)) = pin_of(&mut data) else {
        return Update::DoNothing;
    };
    let update = with_map(&mut app, |s| {
        if let Some((lat, lon)) = s.pins.get(index).copied() {
            s.travel.to = place_text(lat, lon);
            s.travel.to_pick = None;
            s.sidebar_open = true;
            s.selected = None;
            s.announce_travel();
        }
    });
    adopt_field_texts(&mut info);
    ask_route(&app, &mut info);
    update
}

/// A dropped pin's star: the place goes into the favourites, or out of them.
extern "C" fn on_place_favourite(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some((mut app, index)) = pin_of(&mut data) else {
        return Update::DoNothing;
    };
    with_map(&mut app, |s| {
        if let Some((lat, lon)) = s.pins.get(index).copied() {
            lists_ui::toggle_favourite(s, &coords(lat, lon), (lat, lon));
            println!(
                "AZMAPS_FAVOURITE {} {}",
                index,
                if lists_ui::is_favourite(s, (lat, lon)) { "on" } else { "off" }
            );
        }
    })
}

/// A place removed from the recents (and the map).
extern "C" fn on_place_remove(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, index)) = pin_of(&mut data) else {
        return Update::DoNothing;
    };
    let mut removed = false;
    let update = with_map(&mut app, |s| {
        if index < s.pins.len() {
            s.pins.remove(index);
            removed = true;
        }
        s.selected = None;
    });
    if removed {
        save_pins(&app, &mut info);
    }
    update
}

extern "C" fn on_clear_pins(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let update = with_map(&mut data, |s| {
        s.pins.clear();
        s.selected = None;
    });
    save_pins(&data, &mut info);
    update
}

/// Shows or hides the sidebar (kept in settings.json).
extern "C" fn on_toggle_sidebar(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let mut kit_and_value = None;
    let update = with_map(&mut data, |s| {
        s.sidebar_open = !s.sidebar_open;
        let value = if s.sidebar_open { "open" } else { "closed" };
        println!("AZMAPS_SIDEBAR {value}");
        kit_and_value = Some((s.kit.clone(), value));
    });
    if let Some((kit_ref, value)) = kit_and_value {
        kit::set_value(&kit_ref, &mut info, SIDEBAR_KEY, value);
    }
    update
}

/// The field of a `FieldRef` callback: the app and which end.
fn field_of(data: &mut RefAny) -> Option<(RefAny, End)> {
    data.downcast_ref::<FieldRef>().map(|f| (f.app.clone(), f.end))
}

/// A travel field typed in. The keystroke is the field's own edit: the window
/// is not rebuilt for it (that cost a full layout and repaint per key). The
/// text is kept, and once the typing rests the timer tick hands it to the
/// typeahead (`MapState::typeahead`).
extern "C" fn on_travel_text(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let keep = OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    };
    let Some((mut app, end)) = field_of(&mut data) else {
        return keep;
    };
    if let Some(mut s) = app.downcast_mut::<MapState>() {
        *s.travel.text_mut(end) = state.get_text().as_str().to_string();
        s.typed = Some((end, std::time::Instant::now()));
    }
    keep
}

/// The keys of a travel field with the typeahead's list open: Down and Up
/// move the highlight, Return picks the highlighted place, Escape closes the
/// list. Return before the search answered picks its first place once it
/// does.
extern "C" fn on_travel_key(
    mut data: RefAny,
    mut info: CallbackInfo,
    _state: TextInputState,
) -> OnTextInputReturn {
    let keep = |update| OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    };
    let Some((mut app, end)) = field_of(&mut data) else {
        return keep(Update::DoNothing);
    };
    let key = info.get_current_keyboard_state().current_virtual_keycode.into_option();
    let mut picked = false;
    let update = {
        let Some(mut s) = app.downcast_mut::<MapState>() else {
            return keep(Update::DoNothing);
        };
        let rows = s
            .suggestions
            .as_ref()
            .filter(|l| l.end == end)
            .map_or(0, |l| l.list.len());
        match key {
            Some(VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter) if rows > 0 => {
                let index = s.suggestions.as_ref().map_or(0, |l| l.highlighted);
                picked = s.pick(index);
                Update::RefreshDom
            }
            Some(VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter)
                if s.typed.is_some_and(|(typed, _)| typed == end) =>
            {
                s.pick_first = Some(end);
                Update::DoNothing
            }
            Some(VirtualKeyCode::Down | VirtualKeyCode::Up) if rows > 0 => {
                if let Some(list) = s.suggestions.as_mut() {
                    list.highlighted = if matches!(key, Some(VirtualKeyCode::Down)) {
                        (list.highlighted + 1).min(rows - 1)
                    } else {
                        list.highlighted.saturating_sub(1)
                    };
                }
                Update::RefreshDom
            }
            Some(VirtualKeyCode::Escape) if s.suggestions.is_some() => {
                s.suggestions = None;
                Update::RefreshDom
            }
            _ => Update::DoNothing,
        }
    };
    if picked {
        adopt_field_texts(&mut info);
        ask_route(&app, &mut info);
    }
    keep(update)
}

/// The app wrote the travel fields' texts itself (a picked place, a swap): the
/// rebuild's texts replace what the fields hold, edits included.
fn adopt_field_texts(info: &mut CallbackInfo) {
    let revision = info.get_document_text_revision();
    info.mark_text_revision_synced(revision);
}

/// A suggestion's row clicked: its place is picked for its field.
extern "C" fn on_suggestion(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, index)) = data
        .downcast_ref::<SuggestionRef>()
        .map(|r| (r.app.clone(), r.index))
    else {
        return Update::DoNothing;
    };
    let picked = app
        .downcast_mut::<MapState>()
        .is_some_and(|mut s| s.pick(index));
    if !picked {
        return Update::DoNothing;
    }
    adopt_field_texts(&mut info);
    ask_route(&app, &mut info);
    Update::RefreshDom
}

/// The geocoder worker answered: its suggestions show when they answer the
/// newest search for the text the field still holds, and are dropped
/// otherwise. A field whose text stopped naming a place loses its route
/// here too.
extern "C" fn on_search_done(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some((id, end, query, list, compute_ms, complete, offline)) = msg.downcast_mut::<SearchReply>().map(|mut r| {
        (
            r.id,
            r.end,
            std::mem::take(&mut r.query),
            std::mem::take(&mut r.suggestions),
            r.compute_ms,
            r.complete,
            r.offline,
        )
    }) else {
        return Update::DoNothing;
    };
    let picked = {
        let Some(mut s) = app.downcast_mut::<MapState>() else {
            return Update::DoNothing;
        };
        if s.search_latest.load(Ordering::Acquire) != id || s.travel.text(end).trim() != query {
            return Update::DoNothing;
        }
        // Nothing known without the network yet: the list waits for the online search.
        if !complete && list.is_empty() {
            return Update::DoNothing;
        }
        println!("AZMAPS_SUGGEST {} {} {compute_ms:.3}", end.key(), list.len());
        let stage = if offline {
            "offline"
        } else if complete && s.services.transit.is_some() {
            "online"
        } else {
            "local"
        };
        println!("AZMAPS_SUGGEST_SOURCE {} {} {stage}", end.key(), list.len());
        let pick_now = s.pick_first == Some(end) && !list.is_empty();
        s.suggestions = Some(Suggestions {
            end,
            query,
            list,
            highlighted: 0,
            offline,
        });
        s.announce_travel();
        pick_now && s.pick(0)
    };
    if picked {
        adopt_field_texts(&mut info);
    }
    ask_route(&app, &mut info);
    Update::RefreshDom
}

/// The microphone beside the travel fields: the destination is spoken, heard on a worker
/// (azul's speech recognition blocks), and searched like typed text.
extern "C" fn on_speak_destination(app: RefAny, mut info: CallbackInfo) -> Update {
    let request = voice::ListenRequest {
        language: azul_appkit::l10n::locale().chars().take(2).collect(),
        on_done: on_heard,
    };
    println!("AZMAPS_LISTENING");
    info.add_thread(ThreadId::unique(), Thread::create(RefAny::new(request), app, voice::listen_worker));
    Update::DoNothing
}

/// The destination was heard: it is the destination's text, searched once the "typing" rests.
extern "C" fn on_heard(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(heard) = msg.downcast_ref::<voice::Heard>().map(|h| h.clone()) else {
        return Update::DoNothing;
    };
    let update = with_map(&mut app, |s| match &heard.text {
        Some(text) => {
            println!("AZMAPS_HEARD {text}");
            s.travel.to = text.clone();
            s.travel.to_pick = None;
            s.typed = Some((End::To, std::time::Instant::now()));
        }
        None => {
            println!("AZMAPS_NOT_HEARD {}", heard.why);
            s.notice = heard.why.clone();
        }
    });
    adopt_field_texts(&mut info);
    update
}

extern "C" fn on_travel_swap(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let update = with_map(&mut data, |s| {
        // An empty start is "where you are": swapped, it is written out.
        if s.travel.from.trim().is_empty() {
            if let Some((lat, lon)) = s.last_fix {
                s.travel.from = place_text(lat, lon);
            }
        }
        std::mem::swap(&mut s.travel.from, &mut s.travel.to);
        std::mem::swap(&mut s.travel.from_pick, &mut s.travel.to_pick);
        s.suggestions = None;
        s.announce_travel();
    });
    adopt_field_texts(&mut info);
    ask_route(&data, &mut info);
    update
}

extern "C" fn on_travel_mode(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, mode)) = data.downcast_ref::<ModeRef>().map(|m| (m.app.clone(), m.mode))
    else {
        return Update::DoNothing;
    };
    let update = with_map(&mut app, |s| {
        s.travel.mode = mode;
        s.announce_travel();
    });
    ask_route(&app, &mut info);
    update
}

extern "C" fn on_zoom_in(mut data: RefAny, _info: CallbackInfo) -> Update {
    with_map(&mut data, |s| s.zoom_by(1.0))
}

extern "C" fn on_zoom_out(mut data: RefAny, _info: CallbackInfo) -> Update {
    with_map(&mut data, |s| s.zoom_by(-1.0))
}

extern "C" fn on_locate(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let fix = info
        .get_location_fix()
        .into_option()
        .map(|f| (f.latitude_deg, f.longitude_deg));
    let update = with_map(&mut data, |s| {
        s.toggle_locate();
        s.last_fix = fix;
        if let (true, Some((lat, lon))) = (s.locating, fix) {
            s.centre_on(lat, lon);
        }
    });
    // An empty start is where you are: a new fix is a new route.
    ask_route(&data, &mut info);
    update
}

/// The kit's handle, out of the app's state.
fn kit_of(data: &mut RefAny) -> Option<RefAny> {
    data.downcast_ref::<MapState>().map(|s| s.kit.clone())
}

/// The gear: azul-appkit's settings page.
extern "C" fn on_settings_open(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(kit_ref) = kit_of(&mut data) {
        kit::open_settings(&kit_ref, None);
    }
    Update::RefreshDom
}

/// Whether the keyboard focus is in a travel field (or inside one): its
/// arrows and its `-` are the field's, not the map's.
fn typing(info: &CallbackInfo) -> bool {
    let Some(focus) = info.get_focused_node().into_option() else {
        return false;
    };
    let fields: Vec<usize> = [ids::TRAVEL_FROM, ids::TRAVEL_TO]
        .into_iter()
        .map(|id| info.get_node_id_by_id_attribute(DomId { inner: 0 }, id).inner)
        .filter(|node| *node != 0)
        .collect();
    let mut node = Some(focus);
    for _ in 0..4 {
        let Some(n) = node else {
            break;
        };
        if n.dom.inner == 0 && fields.contains(&n.node.inner) {
            return true;
        }
        node = info.get_parent(n).into_option();
    }
    false
}

/// The kit's keys first (Mod+, settings, F1 shortcuts, Escape closes them);
/// then the arrows pan and + / - zoom.
extern "C" fn on_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    if let Some(kit_ref) = kit_of(&mut data) {
        if let Some(update) = kit::handle_key(&kit_ref, &mut info) {
            return update;
        }
        if kit::settings_open(&kit_ref) {
            return Update::DoNothing;
        }
    }
    if typing(&info) {
        return Update::DoNothing;
    }
    let key = info.get_current_keyboard_state().current_virtual_keycode.into_option();
    let card_open = data
        .downcast_ref::<MapState>()
        .is_some_and(|s| s.selected.is_some());
    if key == Some(VirtualKeyCode::Escape) && card_open {
        return with_map(&mut data, |s| s.selected = None);
    }
    let step: fn(&mut MapState) = match key {
        Some(VirtualKeyCode::Left) => |s: &mut MapState| s.pan(-1.0, 0.0),
        Some(VirtualKeyCode::Right) => |s: &mut MapState| s.pan(1.0, 0.0),
        Some(VirtualKeyCode::Up) => |s: &mut MapState| s.pan(0.0, -1.0),
        Some(VirtualKeyCode::Down) => |s: &mut MapState| s.pan(0.0, 1.0),
        Some(VirtualKeyCode::Plus | VirtualKeyCode::Equals | VirtualKeyCode::NumpadAdd) => {
            |s: &mut MapState| s.zoom_by(1.0)
        }
        Some(VirtualKeyCode::Minus | VirtualKeyCode::NumpadSubtract) => {
            |s: &mut MapState| s.zoom_by(-1.0)
        }
        _ => return Update::DoNothing,
    };
    info.prevent_default();
    with_map(&mut data, step)
}

/// The sensors and the location every [`TICK_MS`]; the viewport is kept in
/// settings.json once it has rested `VIEW_SAVE_IDLE_TICKS` ticks. The window
/// is rebuilt only for what shows: the compass when the heading turned by a
/// degree, a new location fix.
extern "C" fn tick(mut data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    // About 3.4 s without a fix: the location is unavailable.
    const LOCATE_TIMEOUT_TICKS: u32 = 34;
    let mag = info
        .callback_info
        .get_sensor_reading(SensorKind::Magnetometer)
        .into_option();
    let fix = info.callback_info.get_location_fix().into_option();
    let mut changed = false;
    let mut moved_fix = false;
    let mut travel_changed = false;
    let mut search = None;
    let mut keep_view = None;
    if let Some(mut s) = data.downcast_mut::<MapState>() {
        // The typeahead: the field's text, once the typing rested.
        if let Some((end, at)) = s.typed {
            if at.elapsed().as_millis() >= TYPEAHEAD_REST_MS {
                s.typed = None;
                let (request, end_changed) = s.typeahead(end);
                search = request;
                travel_changed = end_changed;
                changed |= end_changed;
            }
        }
        if let Some(r) = mag {
            // The compass shows whole degrees: a reading that moves the
            // needle by less is no reason to rebuild the window (every
            // reading used to be).
            let shown = s.heading().map(|h| h.round() as i32);
            if s.has_mag {
                s.mag_x = s.mag_x * 0.8 + r.x * 0.2;
                s.mag_y = s.mag_y * 0.8 + r.y * 0.2;
            } else {
                s.mag_x = r.x;
                s.mag_y = r.y;
                s.has_mag = true;
            }
            changed |= s.heading().map(|h| h.round() as i32) != shown;
        }
        // The platform's location; an external receiver's fixes come through its worker.
        if s.locating && s.drive.source == azul_routing::gnss::Source::Platform {
            match fix {
                Some(f) => {
                    let here = (f.latitude_deg, f.longitude_deg);
                    if s.last_fix != Some(here) {
                        s.last_fix = Some(here);
                        s.centre_on(here.0, here.1);
                        changed = true;
                        moved_fix = true;
                    }
                    s.locate_ticks = 0;
                }
                None => {
                    s.locate_ticks = s.locate_ticks.saturating_add(1);
                    if s.locate_ticks > LOCATE_TIMEOUT_TICKS {
                        s.locating = false;
                        s.locate_failed = true;
                        changed = true;
                    }
                }
            }
        }
        if s.view_dirty {
            s.view_idle_ticks = s.view_idle_ticks.saturating_add(1);
            if s.view_idle_ticks >= VIEW_SAVE_IDLE_TICKS {
                s.view_dirty = false;
                keep_view = Some((
                    s.kit.clone(),
                    view_value(s.viewport.centre_lat_deg, s.viewport.centre_lon_deg, s.viewport.zoom),
                ));
            }
        }
    }
    if let Some((kit_ref, value)) = keep_view {
        kit::set_value(&kit_ref, &mut info.callback_info, VIEW_KEY, &value);
    }
    if let Some(request) = search {
        info.callback_info.add_thread(
            ThreadId::unique(),
            Thread::create(RefAny::new(request), data.clone(), geocode::search_worker),
        );
    }
    if moved_fix || travel_changed {
        // An empty start is where you are: it moved, so did the route. Or a
        // travel field's text came to name a place, or stopped naming one.
        ask_route(&data, &mut info.callback_info);
    }
    // The navigation, once a second; the lists changed since the last tick are written.
    let platform = fix.map(|f| (f.latitude_deg, f.longitude_deg, f.accuracy_m, f.heading_deg, f.speed_mps));
    changed |= drive_ui::tick(&data, &mut info.callback_info, platform);
    lists_ui::save_dirty(&data, &mut info.callback_info);
    TimerCallbackReturn {
        should_terminate: TerminateTimer::Continue,
        should_update: if changed {
            Update::RefreshDom
        } else {
            Update::DoNothing
        },
    }
}

/// The window exists: the `--shot` timer, the sensor tick, the pins file.
extern "C" fn on_window_created(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = data.clone();
    let Some((kit_ref, root, key)) = data
        .downcast_ref::<MapState>()
        .map(|s| (s.kit.clone(), s.data_root.clone(), s.pins_key.clone()))
    else {
        return Update::DoNothing;
    };
    kit::on_window_created(&kit_ref, &mut info);
    info.add_timer(
        TimerId::unique(),
        Timer::create(app.clone(), tick, info.get_system_time_fn())
            .with_interval(Duration::System(SystemTimeDiff::from_millis(TICK_MS))),
    );
    kit::spawn_file_jobs(&mut info, &root, vec![FileJob::Get { key }], app.clone(), TAG_LOAD, on_files_done);
    lists_ui::load(&app, &mut info);
    drive_ui::start_source(&app, &mut info);
    Update::DoNothing
}

// ==== Entry ====

/// The map's tile layer: the widget's own, on `template`'s server when one is named
/// ([`args::tile_template`]).
fn tile_layer(template: Option<String>) -> MapTileLayer {
    let mut layer = MapTileLayer::default();
    if let Some(template) = template {
        layer.url_template = AzString::from(template);
    }
    layer
}

pub fn start() {
    let stats_env =
        std::env::var("AZMAPS_STATS").is_ok_and(|v| !v.trim().is_empty() && v.trim() != "0");
    let args = match args::parse(std::env::args().skip(1), stats_env) {
        Ok(a) if a.help => {
            println!("{}", args::usage());
            std::process::exit(0);
        }
        Ok(a) => a,
        Err(why) => {
            eprintln!("{why}");
            std::process::exit(2);
        }
    };
    if args.stats {
        STATS.store(true, Ordering::Relaxed);
        // The map widget's own switch: its counters and its tile worker's.
        // Set before the app starts a single thread.
        std::env::set_var("AZ_MAP_STATS", "1");
    }
    let kit_ref = kit::create_kit(SPEC, ABOUT, &SHORTCUTS, &APP_CATEGORIES, args.kit.clone());
    // The words first: everything from here is said in the language chosen (Settings,
    // `--language`; the system's otherwise).
    let mut config = kit::app_config(&kit_ref);
    crate::l10n::register(&mut config);
    let mut kit_now = kit_ref.clone();
    let language = kit_now.downcast_ref::<kit::Kit>().map(|k| k.language());
    if let Some(language) = language.filter(|l| *l != azul_appkit::args::LanguagePref::System) {
        azul_appkit::l10n::set_locale(language.tag());
    }
    let (data_root, pins_key, kept, sidebar_open) = {
        let mut k = kit_ref.clone();
        let read = match k.downcast_ref::<kit::Kit>() {
            Some(k) => (
                k.data_root.clone(),
                k.key(PINS_FILE),
                k.settings.get(VIEW_KEY).and_then(|text| parse_view(text)),
                k.settings.get(SIDEBAR_KEY) != Some("closed"),
            ),
            None => (PathBuf::new(), String::new(), None, true),
        };
        read
    };
    // The tile server: `--tiles`, `AZMAPS_TILES`, the shared config's `endpoints.tiles` (the
    // file the kit read; none for a `--shot` run), else the widget's own.
    let shared_tiles = {
        let mut k = kit_ref.clone();
        let found = k
            .downcast_ref::<kit::Kit>()
            .and_then(|k| k.config_path.clone());
        found
    }
    .and_then(|path| azul_appkit::shared_endpoint::in_file(&path, "tiles"));
    let tiles = args::tile_template(
        args.tiles.as_deref(),
        std::env::var(args::TILES_VAR).ok().as_deref(),
        shared_tiles.as_deref(),
    );
    // The routing tiles: `--routing`, `AZMAPS_ROUTING`, the shared config's `endpoints.routing`;
    // without them a route is the straight-line estimate. Downloads are cached in the data
    // folder (`maps/routing/`).
    let shared_routing = {
        let mut k = kit_ref.clone();
        let found = k
            .downcast_ref::<kit::Kit>()
            .and_then(|k| k.config_path.clone());
        found
    }
    .and_then(|path| azul_appkit::shared_endpoint::in_file(&path, "routing"));
    let routing_template = args::tile_template(
        args.routing.as_deref(),
        std::env::var(args::ROUTING_VAR).ok().as_deref(),
        shared_routing.as_deref(),
    );
    if let Some(template) = &routing_template {
        println!("AZMAPS_ROUTING {template}");
    }
    // Settings > Navigation > Routing tiles: how long downloaded roads are kept.
    let keep_days = {
        let mut k = kit_ref.clone();
        let found = k
            .downcast_ref::<kit::Kit>()
            .and_then(|k| k.settings.get(azul_routing::source::SETTINGS_KEY).map(str::to_string));
        found
    };
    let routing = routing_template.map(|t| {
        route::routing_source_keeping(&t, &data_root.join("maps").join("routing"), keep_days.as_deref())
    });
    // Without routing tiles a route is asked of the routing server (FOSSGIS's
    // public Valhalla unless `--routing-online` / AZMAPS_ROUTING_ONLINE names
    // another or says off).
    // The online services and the one cache of their answers (`maps/cache/`): the routing
    // server's routes, Transitous' journeys and places answer again offline.
    let transit_url = args::transit_server(args.transit.as_deref(), std::env::var(args::TRANSIT_VAR).ok().as_deref());
    let services = crate::services::Services::new(&data_root, transit_url.as_deref(), args.offline);
    if let Some(server) = &services.transit {
        println!("AZMAPS_TRANSIT {}", server.base);
    }
    if args.offline {
        println!("AZMAPS_OFFLINE");
    }
    let online = args::online_routing(
        args.routing_online.as_deref(),
        std::env::var(args::ROUTING_ONLINE_VAR).ok().as_deref(),
    )
    .map(|url| route::online_source_cached(&url, Arc::clone(&services.cache), args.offline));
    let transit_provider = services
        .transit
        .clone()
        .map(|server| server as Arc<dyn azul_routing::transit::TransitProvider>);
    // Where the position comes from: `--gps`, else Settings > Location, else the platform's.
    let source_setting = {
        let mut k = kit_ref.clone();
        let found = k
            .downcast_ref::<kit::Kit>()
            .and_then(|k| k.settings.get(location::SOURCE_KEY).map(str::to_string));
        found
    };
    let source = location::source_of(args.gps.as_deref(), source_setting.as_deref());
    let simulate_mps = args
        .simulate
        .then(|| args.simulate_kmh.unwrap_or(args::SIMULATED_KMH) / 3.6);
    if let (None, Some(server)) = (&routing, &online) {
        println!("AZMAPS_ROUTING_ONLINE {}", server.url);
    }
    // The map opens where it was left (settings.json), else on the start.
    let (lat, lon, zoom) = kept.unwrap_or(HOME);
    println!("{}", view_line(lat, lon, zoom));
    let state = MapState {
        viewport: MapViewport {
            centre_lat_deg: lat,
            centre_lon_deg: lon,
            zoom,
            bearing_deg: 0.0,
            pitch_deg: 0.0,
        },
        layer: tile_layer(tiles),
        locating: false,
        last_fix: None,
        pins: Vec::new(),
        selected: None,
        sidebar_open,
        travel: Travel::default(),
        mag_x: 0.0,
        mag_y: 0.0,
        has_mag: false,
        locate_failed: false,
        locate_ticks: 0,
        view_dirty: false,
        view_idle_ticks: 0,
        kit: kit_ref.clone(),
        data_root,
        pins_key,
        saving: false,
        save_pending: false,
        notice: String::new(),
        size: Cell::new((0.0, 0.0)),
        route: None,
        route_progress: None,
        offline_areas: offline_ui::OfflineState::new(routing.as_ref()),
        routing,
        online,
        selected_step: None,
        route_asked: None,
        route_latest: Arc::new(AtomicU64::new(0)),
        typed: None,
        suggestions: None,
        pick_first: None,
        search_latest: Arc::new(AtomicU64::new(0)),
        services,
        transit: transit_panel::TransitState::new(transit_provider),
        drive: drive_ui::DriveState::new(source, simulate_mps, args.navigate, args.replay_speed.unwrap_or(1.0)),
        lists: lists_ui::ListsState::new(
            |name| azul_appkit::data::app_key(ABOUT.app_folder, name),
            args.import.clone(),
        ),
    };
    let app = App::create(RefAny::new(state), config);
    let window =
        kit::window_options(&kit_ref, layout, (1100.0, 720.0), (640.0, 420.0), on_window_created);
    app.run(window);
}

#[cfg(test)]
mod travel_line_tests {
    use super::*;

    fn two_places() -> MapLatLonVec {
        MapLatLonVec::from_vec(vec![at((48.2082, 16.3738)), at((48.1372, 11.5756))])
    }

    #[test]
    fn the_road_route_and_its_preview_are_drawn_in_the_themes_accent() {
        let accent = SystemColorRef::Accent.to_color_token();
        let road = travel_polyline(two_places(), true, TravelMode::Car);
        let preview = travel_polyline(two_places(), false, TravelMode::Car);
        assert_eq!(road.color, accent, "the road route follows the theme");
        assert_eq!(preview.color, accent, "so does the straight preview");
        assert!(road.casing_width > road.width, "the road route has an outline under it");
        assert!(preview.width < road.width, "the preview is the thinner line");
    }

    #[test]
    fn a_walking_route_is_drawn_dotted_and_a_drive_or_a_ride_solid() {
        let walk = travel_polyline(two_places(), true, TravelMode::Walk);
        assert_eq!(walk.dash, MapLineDash::Dotted, "a walk is dots, as on every map");
        assert!(walk.casing_width > walk.width, "each dot has its outline");
        for mode in [TravelMode::Car, TravelMode::Bike] {
            assert_eq!(travel_polyline(two_places(), true, mode).dash, MapLineDash::Solid, "{mode:?}");
        }
    }
}

#[cfg(test)]
mod title_area_tests {
    use super::*;

    #[test]
    fn the_title_area_keeps_the_window_controls_room_on_the_side_they_sit() {
        let mac = title_area_css(kit::TitleInsets { left: 78.0, right: 0.0, height: 28.0 });
        assert!(mac.contains("padding: 0px 10px 0px 88px;"), "{mac}");
        let windows = title_area_css(kit::TitleInsets { left: 0.0, right: 138.0, height: 32.0 });
        assert!(windows.contains("padding: 0px 148px 0px 10px;"), "the caption buttons keep their room: {windows}");
    }

    #[test]
    fn the_title_area_is_at_least_as_tall_as_its_buttons_and_the_window_controls() {
        assert!(title_area_css(kit::TitleInsets { left: 0.0, right: 0.0, height: 28.0 }).contains("height: 40px;"));
        assert!(title_area_css(kit::TitleInsets { left: 0.0, right: 0.0, height: 46.0 }).contains("height: 46px;"));
    }
}

#[cfg(test)]
mod map_setup_tests {
    use super::*;

    #[test]
    fn the_map_keeps_its_tiles_in_the_data_folder_and_ships_the_world() {
        let root = PathBuf::from("/data/azmaps");
        let setup = map_setup(&root, &MapTileLayer::default(), MapSetup::create());
        assert_eq!(
            PathBuf::from(setup.disk_cache_dir.as_str()),
            root.join("maps").join("tiles"),
            "downloaded tiles are kept in the data folder across restarts"
        );
        assert_eq!(setup.seed_tiles.len(), 5, "the world at zoom 0 and 1 is shipped");
    }

    #[test]
    fn another_tile_server_gets_no_shipped_tiles_and_no_data_folder_no_disk_cache() {
        let mut layer = MapTileLayer::default();
        layer.url_template = AzString::from("http://localhost:8080/{z}/{x}/{y}.pbf");
        let setup = map_setup(&PathBuf::from("/data/azmaps"), &layer, MapSetup::create());
        assert_eq!(setup.seed_tiles.len(), 0, "the seeds are OpenFreeMap's tiles only");
        let setup = map_setup(&PathBuf::new(), &MapTileLayer::default(), MapSetup::create());
        assert!(setup.disk_cache_dir.as_str().is_empty(), "no data folder: memory only");
    }

    #[test]
    fn the_shipped_tiles_are_the_vector_tiles_of_zoom_0_and_1() {
        let mut ids: Vec<(u8, u32, u32)> = SEED_TILES.iter().map(|(z, x, y, _)| (*z, *x, *y)).collect();
        ids.sort_unstable();
        assert_eq!(ids, vec![(0, 0, 0), (1, 0, 0), (1, 0, 1), (1, 1, 0), (1, 1, 1)]);
        for (z, x, y, bytes) in SEED_TILES {
            assert!(bytes.len() > 1000, "{z}/{x}/{y}: {} bytes", bytes.len());
            assert_eq!(bytes[0], 0x1a, "{z}/{x}/{y} starts with a layer: a vector tile");
        }
    }
}

#[cfg(test)]
mod engine_feature_tests {
    const MANIFEST: &str = include_str!("../Cargo.toml");

    fn azul_dll_dependency_lines() -> Vec<&'static str> {
        MANIFEST
            .lines()
            .map(str::trim)
            .filter(|l| !l.starts_with('#'))
            .filter(|l| l.contains(r#"package = "azul-dll""#))
            .collect()
    }

    #[test]
    fn the_demo_asks_the_engine_for_the_tile_pipeline_on_every_target() {
        let deps = azul_dll_dependency_lines();
        assert!(
            !deps.is_empty(),
            "this manifest declares no azul-dll dependency at all — the selector below is stale, \
             not the manifest"
        );

        for dep in &deps {
            assert!(
                dep.contains("\"map-tiles\""),
                "an azul-dll dependency of AzMaps does not enable `map-tiles`:\n  {dep}\nWithout \
                 it `map_widget_dom` compiles its `#[cfg(not(feature = \"map-tiles\"))]` half, \
                 which returns the placeholder DOM and wires NO tile-fetch worker — the demo pans \
                 a permanently empty grid. Enabling it here is what makes the demo correct in \
                 BOTH link modes: it is harmless when the dylib supplies the worker, and it is \
                 the only thing that supplies it when cargo unifies `cabi_internal` in and the \
                 engine gets compiled into this binary instead."
            );
        }
    }
}

#[cfg(target_os = "android")]
#[ctor::ctor]
fn azul_android_init() {
    start();
}
