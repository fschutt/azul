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
//! A place's card is a transient popover anchored at its pin (Directions,
//! Remove). The travel panel takes a start (empty: where you are, once the
//! location is known) and a destination as `lat, lon`, a mode (drive, walk,
//! cycle, public transport) and draws the straight line between them with its
//! distance - the routing itself is not built yet (ROUTING.md).
//!
//! Pins are kept in the data tree (`maps/pins.json`, through azul-appkit's
//! file jobs on a Thread); the last viewport and the sidebar in settings.json.
//! azul-appkit gives the switches (`--theme`, `--mode`, `--size`, `--shot`,
//! `--data-dir`) and the settings page (the gear, Mod+,). Arrows pan, `+` /
//! `-` zoom (not while a travel field is being typed in).
//!
//! stdout, for scripts (`scripts/azmaps_e2e.py`): `AZMAPS_VIEW <lat> <lon>
//! <zoom>` on every viewport change, `AZMAPS_PINS <n>` when the pins change,
//! `AZMAPS_PINS_LOADED <n>` / `AZMAPS_PINS_SAVED <n>` / `AZMAPS_PINS_ERROR`,
//! `AZMAPS_PLACE <index>` when a place's card opens, `AZMAPS_TRAVEL <mode>
//! <from> <to>` when the travel panel changes, `AZMAPS_SIDEBAR open|closed`.

pub mod ids;
pub mod model;

use std::path::PathBuf;

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, PopoverOnToggleCallbackType, TextInputOnTextInputCallbackType,
        TimerCallbackInfo, TimerCallbackReturn,
    },
    dom::{DomId, GeolocationProbeConfig, VirtualKeyCode},
    prelude::*,
    sensor::SensorKind,
    shells::{ShellThemeAccent, ShellThemeScope},
    str::String as AzString,
    task::TerminateTimer,
    widgets::{
        Button, ButtonType, MapLatLon, MapTileLayer, MapViewport, MapWidget, OnTextInputReturn,
        Popover, PopoverState, TextInputState, TextInputValid,
    },
};
use azul_appkit::{
    about::AboutInfo,
    args::{AppArgs, AppSpec},
    files::{FileJob, FileOutcome},
    shortcuts::Shortcut,
    ui as kit,
};

use crate::model::{
    cardinal, clip_segment, distance_km, distance_text, pan_tiles, parse_place, parse_view,
    pins_from_json, pins_to_json, place_text, travel_line, view_line, view_value, TravelMode,
    HOME, MAX_ZOOM, MIN_ZOOM, PINS_FILE, SIDEBAR_KEY, VIEW_KEY,
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
    Shortcut::new("Map", "Left", "Pan west"),
    Shortcut::new("Map", "Right", "Pan east"),
    Shortcut::new("Map", "Up", "Pan north"),
    Shortcut::new("Map", "Down", "Pan south"),
    Shortcut::new("Map", "+", "Zoom in"),
    Shortcut::new("Map", "-", "Zoom out"),
];

/// The write-back tags of AzMaps' file jobs.
const TAG_LOAD: u64 = 1;
const TAG_SAVE: u64 = 2;

/// Timer ticks without a viewport change before the viewport is kept (a
/// drag or a held key is one write, not one per frame).
const VIEW_SAVE_IDLE_TICKS: u32 = 30;

/// A tap on the map this soon after the user closed a place's card is the
/// click that closed it, not a new pin.
const DISMISS_TAP_MS: u128 = 600;

/// The pin's head: a 20 px square, round but for its bottom-left corner,
/// turned 45 degrees COUNTER-clockwise so that corner points straight down.
/// (It was `rotate(45deg)`: CSS turns clockwise, which put the point on
/// the left - every pin lay on its side.) The point is 10 * sqrt(2) px
/// below the square's centre.
const PIN_HEAD: &str = "width: 20px; height: 20px; border-radius: 10px 10px 10px 0px; background: \
                        #e5322d; transform: rotate(-45deg); box-shadow: 0px 1px 3px \
                        rgba(0,0,0,0.45); display: flex; align-items: center; justify-content: \
                        center; cursor: pointer;";
const PIN_DOT: &str = "width: 7px; height: 7px; border-radius: 4px; background: #ffffff;";
/// Where the pin's box sits relative to its place: half its width to the
/// left, its centre 10 * sqrt(2) px above the point.
const PIN_HALF_WIDTH: f32 = 10.0;
const PIN_TIP_DEPTH: f32 = 24.1;

/// The window's root: the positioning box everything floats in.
const ROOT: &str =
    "position: relative; flex-grow: 1; min-height: 0px; overflow: hidden; display: flex;";
/// The map: the whole window. Ground under the tiles; the tiles follow the
/// mode themselves.
const MAP_AREA: &str = "position: absolute; left: 0px; top: 0px; right: 0px; bottom: 0px; \
                        background: #cbd2d8; overflow: hidden;";
/// The title area over the map: the window's drag region; the traffic
/// lights sit in its left end on macOS.
#[cfg(target_os = "macos")]
const TITLE_AREA: &str = "position: absolute; left: 0px; top: 0px; right: 0px; height: 40px; \
                          display: flex; flex-direction: row; align-items: center; gap: 6px; \
                          padding: 0px 10px 0px 84px; -azul-app-region: drag;";
#[cfg(not(target_os = "macos"))]
const TITLE_AREA: &str = "position: absolute; left: 0px; top: 0px; right: 0px; height: 40px; \
                          display: flex; flex-direction: row; align-items: center; gap: 6px; \
                          padding: 0px 10px 0px 10px; -azul-app-region: drag;";
/// A control in the drag region keeps its clicks.
const NO_DRAG: &str = "-azul-app-region: no-drag;";
/// A floating group of map controls (Apple Maps' rounded pills).
const CONTROL_GROUP: &str = "display: flex; flex-direction: column; background: \
                             system:window-background; border-radius: 8px; box-shadow: 0px 1px \
                             4px rgba(0,0,0,0.3); -azul-app-region: no-drag;";
/// The controls' column, bottom right.
const CONTROLS: &str = "position: absolute; right: 12px; bottom: 30px; display: flex; \
                        flex-direction: column; gap: 10px;";
/// The floating sidebar, below the title area.
const SIDEBAR: &str = "position: absolute; left: 10px; top: 44px; bottom: 26px; width: 290px; \
                       display: flex; flex-direction: column; gap: 12px; padding: 12px; \
                       background: system:window-background; color: system:text; \
                       border-radius: 12px; box-shadow: 0px 2px 10px rgba(0,0,0,0.25);";
const COLUMN: &str = "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;";
const TRAVEL_PANEL: &str = "display: flex; flex-direction: column; gap: 6px;";
const ROW: &str = "display: flex; flex-direction: row; align-items: center; gap: 6px;";
const FIELDS: &str =
    "display: flex; flex-direction: column; gap: 6px; flex-grow: 1; min-width: 0px;";
const FIELD_GROW: &str = "flex-grow: 1; min-width: 0px;";
const FROM_MARK: &str = "width: 10px; height: 10px; border-radius: 6px; border: 2px solid \
                         #1a73e8; background: #ffffff;";
const TO_MARK: &str = "width: 10px; height: 10px; border-radius: 6px; border: 2px solid \
                       #e5322d; background: #e5322d;";
const DISTANCE: &str = "flex-grow: 1; text-align: right; font-size: 13px; color: \
                        system:secondary-text; white-space: nowrap;";
const RECENTS_SECTION: &str =
    "display: flex; flex-direction: column; gap: 4px; flex-grow: 1; min-height: 0px;";
const SECTION_HEAD: &str = "display: flex; flex-direction: row; align-items: center; \
                            font-size: 12px; font-weight: bold; color: system:secondary-text;";
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
                           border-radius: 3px;";
/// A problem with the pins file, as a small toast.
const NOTICE: &str = "position: absolute; left: 50%; bottom: 30px; width: 360px; margin-left: \
                      -180px; text-align: center; font-size: 12px; color: #ffffff; background: \
                      rgba(176,0,32,0.9); padding: 6px 10px; border-radius: 6px;";
const LOCATION_DOT: &str = "position: absolute; width: 16px; height: 16px; margin-left: -8px; \
                            margin-top: -8px; background: #4285f4; border-radius: 8px; \
                            box-shadow: 0px 0px 0px 3px rgba(66,133,244,0.35);";
const COMPASS_BADGE: &str = "position: absolute; right: 12px; top: 50px; width: 44px; height: \
                             44px; border-radius: 22px; background: rgba(20,20,28,0.85); border: \
                             2px solid #6a7080; display: flex; align-items: center; \
                             justify-content: center; box-shadow: 0px 1px 4px rgba(0,0,0,0.4);";
const NEEDLE_N: &str = "flex-grow: 1; background: #e74c3c; border-radius: 4px 4px 0px 0px;";
const NEEDLE_S: &str = "flex-grow: 1; background: #cfd2d8; border-radius: 0px 0px 4px 4px;";
/// The travel preview: a straight line from the start to the destination.
const ROUTE_COLOR: &str = "#1a73e8";

// ==== State ====

/// The travel panel: two places as typed, and how to go.
#[derive(Default)]
struct Travel {
    from: String,
    to: String,
    mode: TravelMode,
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
    /// When the user last closed a place's card (see [`DISMISS_TAP_MS`]).
    dismissed_at: Option<std::time::Instant>,
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
}

impl MapState {
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
            parse_place(&self.travel.from)
        };
        (from, parse_place(&self.travel.to))
    }

    /// The travel panel changed: announced for scripts.
    fn announce_travel(&self) {
        let (from, to) = self.travel_ends();
        println!("{}", travel_line(self.travel.mode, from, to));
    }
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
        .with_accessibility_name(name)
}

/// The title area over the map: the window moves by it; the sidebar's
/// button on the left, the settings on the right, no text.
fn title_area(s: &MapState, app: &RefAny) -> Dom {
    let toggle_name = if s.sidebar_open { "Hide sidebar" } else { "Show sidebar" };
    Dom::create_div()
        .with_css(TITLE_AREA)
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
                .with_child(tool(app, "settings", "Settings", ids::SETTINGS, on_settings_open)),
        )
}

/// What a travel field is about.
#[derive(Clone, Copy)]
enum End {
    From,
    To,
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
                        .with_placeholder(placeholder)
                        .with_accessibility_name(name)
                        .with_on_text_input(
                            RefAny::new(FieldRef {
                                app: app.clone(),
                                end,
                            }),
                            on_travel_text as TextInputOnTextInputCallbackType,
                        )
                        .dom()
                        .with_id(id),
                ),
            )
    };
    let from_placeholder = if s.last_fix.is_some() { "My Location" } else { "From" };
    let fields = Dom::create_div()
        .with_css(FIELDS)
        .with_child(field(
            FROM_MARK,
            &s.travel.from,
            from_placeholder,
            "Start",
            ids::TRAVEL_FROM,
            End::From,
        ))
        .with_child(field(
            TO_MARK,
            &s.travel.to,
            "To",
            "Destination",
            ids::TRAVEL_TO,
            End::To,
        ));
    let ends = Dom::create_div().with_css(ROW).with_child(fields).with_child(tool(
        app,
        "swap_vert",
        "Swap start and destination",
        ids::TRAVEL_SWAP,
        on_travel_swap,
    ));

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
                .with_accessibility_name(mode.label()),
        );
    }
    if let (Some(a), Some(b)) = s.travel_ends() {
        modes.add_child(
            Dom::create_div()
                .with_css(DISTANCE)
                .with_id(ids::TRAVEL_DISTANCE)
                .with_child(Dom::create_span_with_text(distance_text(distance_km(a, b)))),
        );
    }

    Dom::create_div()
        .with_css(TRAVEL_PANEL)
        .with_id(ids::TRAVEL)
        .with_accessibility_name("Directions")
        .with_child(ends)
        .with_child(modes)
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
        .with_accessibility_name("Recents");
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
                            .with_child(Dom::create_span_with_text("Recents")),
                    )
                    .with_child(tool(
                        app,
                        "delete_sweep",
                        "Clear recents",
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
        .with_accessibility_name("Sidebar")
        .with_child(travel_panel(s, app));
    if let Some(list) = recents(s, app) {
        side.add_child(list);
    }
    side
}

/// A place's card, in the popover at its pin: what it is, where it is,
/// directions to it, remove it.
fn place_card(app: &RefAny, index: usize, lat: f64, lon: f64) -> Dom {
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
                .with_child(Dom::create_span_with_text("Dropped Pin")),
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
                    Button::with_type("Directions", ButtonType::Primary)
                        .with_icon("directions")
                        .with_on_click(pin(), on_place_directions as ButtonOnClickCallbackType)
                        .dom()
                        .with_id(ids::indexed("place-directions", index)),
                )
                .with_child(
                    Button::create("")
                        .with_icon("delete")
                        .with_on_click(pin(), on_place_remove as ButtonOnClickCallbackType)
                        .dom()
                        .with_id(ids::indexed("place-remove", index))
                        .with_accessibility_name("Remove"),
                ),
        )
}

/// A place on the map: its pin, the anchor of its card's popover, with the
/// pin's point on the place.
fn place_pin(s: &MapState, app: &RefAny, index: usize, x: f32, y: f32) -> Dom {
    let (lat, lon) = s.pins[index];
    let marker = Dom::create_div()
        .with_css(PIN_HEAD)
        .with_child(Dom::create_div().with_css(PIN_DOT))
        .with_id(ids::indexed("place-pin", index))
        .with_accessibility_name(coords(lat, lon));
    let popover = Popover::create(marker, place_card(app, index, lat, lon))
        .with_open(s.selected == Some(index))
        .with_on_toggle(
            RefAny::new(PinRef {
                app: app.clone(),
                index,
            }),
            on_place_toggle as PopoverOnToggleCallbackType,
        )
        .dom();
    let at = format!(
        "position: absolute; left: {:.1}px; top: {:.1}px;",
        x - PIN_HALF_WIDTH,
        y - PIN_TIP_DEPTH,
    );
    Dom::create_div().with_css(at.as_str()).with_child(popover)
}

/// The travel preview: the straight line from `a` to `b` (both in view
/// pixels, already clipped to the view).
fn route_line(a: (f32, f32), b: (f32, f32)) -> Dom {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let css = format!(
        "position: absolute; left: {:.1}px; top: {:.1}px; width: {:.1}px; height: 4px; \
         margin-top: -2px; background: {ROUTE_COLOR}; border-radius: 2px; opacity: 0.85; \
         transform-origin: 0px 50%; transform: rotate({:.2}deg);",
        a.0,
        a.1,
        dx.hypot(dy),
        dy.atan2(dx).to_degrees(),
    );
    Dom::create_div().with_css(css.as_str())
}

/// A small round mark on the map (the start of a route).
fn map_dot(x: f32, y: f32, css: &str) -> Dom {
    let at = format!(
        "position: absolute; left: {:.1}px; top: {:.1}px; margin-left: -7px; margin-top: -7px; \
         {css}",
        x, y
    );
    Dom::create_div().with_css(at.as_str())
}

/// The map: the whole window - the tiles, the travel preview, the places,
/// where you are, the compass.
fn map_area(s: &MapState, app: &RefAny, size: (f32, f32)) -> Dom {
    let map = MapWidget::create(s.layer.clone())
        .with_viewport(s.viewport)
        .with_on_viewport_changed(app.clone(), on_viewport_changed)
        .with_on_pin_tap(app.clone(), on_pin_tap)
        .dom();
    let mut area = Dom::create_div()
        .with_css(MAP_AREA)
        .with_id(ids::MAP)
        .with_child(map);

    // The map fills the window, so the window's size is the map's: where a
    // place is on screen.
    let (w, h) = size;
    let at = |lat: f64, lon: f64| {
        let p = MapWidget::px_at_latlon(
            s.viewport,
            MapLatLon {
                lat_deg: lat,
                lon_deg: lon,
            },
            LogicalSize::create(w, h),
        );
        (p.x, p.y)
    };
    let visible = |(x, y): (f32, f32)| x > -40.0 && x < w + 40.0 && y > -40.0 && y < h + 60.0;

    // The travel preview, until there is routing: start to destination as
    // the crow flies.
    if let (Some(a), Some(b)) = s.travel_ends() {
        let (pa, pb) = (at(a.0, a.1), at(b.0, b.1));
        if let Some((p, q)) = clip_segment(pa, pb, w, h) {
            area.add_child(route_line(p, q));
        }
        if visible(pa) {
            area.add_child(map_dot(pa.0, pa.1, FROM_MARK));
        }
        if visible(pb) {
            area.add_child(map_dot(pb.0, pb.1, TO_MARK));
        }
    }

    if s.locating {
        area.add_child(Dom::create_geolocation_probe(GeolocationProbeConfig {
            high_accuracy: true,
            background: false,
            max_accuracy_m: 0.0,
            min_interval_ms: 0,
        }));
        if let Some((lat, lon)) = s.last_fix {
            let (x, y) = at(lat, lon);
            if visible((x, y)) {
                let dot = format!("{LOCATION_DOT} left: {x:.1}px; top: {y:.1}px;");
                area.add_child(
                    Dom::create_div()
                        .with_css(dot.as_str())
                        .with_accessibility_name("You are here"),
                );
            }
        }
    }

    for (index, &(lat, lon)) in s.pins.iter().enumerate() {
        let (x, y) = at(lat, lon);
        if visible((x, y)) {
            area.add_child(place_pin(s, app, index, x, y));
        }
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
                .with_accessibility_name(format!(
                    "Heading {} {heading:03.0}\u{b0}",
                    cardinal(heading)
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
        ("location_disabled", "Location unavailable")
    } else {
        ("my_location", "Show my location")
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
                .with_child(tool(app, "add", "Zoom in", ids::ZOOM_IN, on_zoom_in))
                .with_child(tool(app, "remove", "Zoom out", ids::ZOOM_OUT, on_zoom_out)),
        )
}

/// The map screen: the map, and what floats on it.
fn screen(s: &MapState, app: &RefAny, size: (f32, f32)) -> Dom {
    let mut root = Dom::create_div()
        .with_css(ROOT)
        .with_child(map_area(s, app, size))
        .with_child(title_area(s, app));
    if s.sidebar_open {
        root.add_child(sidebar(s, app));
    }
    root.add_child(map_controls(s, app));
    root.add_child(
        Dom::create_div()
            .with_css(ATTRIBUTION)
            .with_child(Dom::create_span_with_text(s.layer.attribution.as_str())),
    );
    if !s.notice.is_empty() {
        root.add_child(
            Dom::create_div()
                .with_css(NOTICE)
                .with_child(Dom::create_span_with_text(s.notice.as_str())),
        );
    }
    root
}

extern "C" fn layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    // Reading the mode makes a light / dark switch rebuild the window (the
    // tiles' style follows it).
    let _mode = info.get_mode();
    let size = (info.get_window_width(), info.get_window_height());
    let app = data.clone();
    let Some(s) = data.downcast_ref::<MapState>() else {
        return Dom::create_body();
    };
    let content = if kit::settings_open(&s.kit) {
        // azul-appkit's settings page: Appearance, Data, Shortcuts, About.
        Dom::create_div()
            .with_css(COLUMN)
            .with_child(kit::title_row(SPEC.name))
            .with_child(kit::settings_page(&s.kit, Vec::new()))
    } else {
        screen(&s, &app, size)
    };
    let column = Dom::create_div().with_css(COLUMN).with_child(content);
    // The scope as the window's body: no UA margin, the full window height.
    ShellThemeScope::create(column)
        .with_accent(ShellThemeAccent::Leaf)
        .body()
        .with_callback(EventFilter::Window(WindowEventFilter::VirtualKeyDown), app, on_key)
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
                                s.notice = format!("The pins file could not be read: {why}");
                            }
                        }
                    }
                    FileOutcome::Got { result: Ok(None), .. } => println!("AZMAPS_PINS_LOADED 0"),
                    other => {
                        let why = other.error().unwrap_or_default();
                        println!("AZMAPS_PINS_ERROR {why}");
                        s.notice = format!("The pins file could not be read: {why}");
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
                    s.notice = format!("The pins could not be saved: {why}");
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

extern "C" fn on_viewport_changed(mut data: RefAny, _info: CallbackInfo, vp: MapViewport) -> Update {
    with_map(&mut data, |s| {
        s.viewport = vp;
        s.moved();
    })
}

/// A tap on the map drops a pin - unless it is the click that just closed a
/// place's card (that click also lands on the map under it).
extern "C" fn on_pin_tap(mut data: RefAny, mut info: CallbackInfo, coord: MapLatLon) -> Update {
    let mut dropped = false;
    let update = with_map(&mut data, |s| {
        let closing_click = s
            .dismissed_at
            .take()
            .is_some_and(|t| t.elapsed().as_millis() < DISMISS_TAP_MS);
        if closing_click || s.selected.is_some() {
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
extern "C" fn on_place_row(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some((mut app, index)) = pin_of(&mut data) else {
        return Update::DoNothing;
    };
    with_map(&mut app, |s| {
        if let Some((lat, lon)) = s.pins.get(index).copied() {
            s.centre_on(lat, lon);
            s.selected = Some(index);
            println!("AZMAPS_PLACE {index}");
        }
    })
}

/// A place's card opened (its pin clicked) or closed (by the user: a click
/// outside it, Escape, the pin again).
extern "C" fn on_place_toggle(
    mut data: RefAny,
    _info: CallbackInfo,
    state: PopoverState,
) -> Update {
    let Some((mut app, index)) = pin_of(&mut data) else {
        return Update::DoNothing;
    };
    with_map(&mut app, |s| {
        if state.open {
            if s.selected != Some(index) {
                println!("AZMAPS_PLACE {index}");
            }
            s.selected = Some(index);
        } else if s.selected == Some(index) {
            s.selected = None;
            s.dismissed_at = Some(std::time::Instant::now());
        }
    })
}

/// Directions to a place: it becomes the destination, the sidebar shows the
/// travel panel, the card closes.
extern "C" fn on_place_directions(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some((mut app, index)) = pin_of(&mut data) else {
        return Update::DoNothing;
    };
    with_map(&mut app, |s| {
        if let Some((lat, lon)) = s.pins.get(index).copied() {
            s.travel.to = place_text(lat, lon);
            s.sidebar_open = true;
            s.selected = None;
            s.announce_travel();
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

/// A travel field typed in.
extern "C" fn on_travel_text(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let keep = OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    };
    let Some((mut app, end)) = data.downcast_ref::<FieldRef>().map(|f| (f.app.clone(), f.end))
    else {
        return keep;
    };
    let text = state.get_text().as_str().to_string();
    let update = with_map(&mut app, |s| {
        match end {
            End::From => s.travel.from = text,
            End::To => s.travel.to = text,
        }
        s.announce_travel();
    });
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

extern "C" fn on_travel_swap(mut data: RefAny, _info: CallbackInfo) -> Update {
    with_map(&mut data, |s| {
        // An empty start is "where you are": swapped, it is written out.
        if s.travel.from.trim().is_empty() {
            if let Some((lat, lon)) = s.last_fix {
                s.travel.from = place_text(lat, lon);
            }
        }
        std::mem::swap(&mut s.travel.from, &mut s.travel.to);
        s.announce_travel();
    })
}

extern "C" fn on_travel_mode(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some((mut app, mode)) = data.downcast_ref::<ModeRef>().map(|m| (m.app.clone(), m.mode))
    else {
        return Update::DoNothing;
    };
    with_map(&mut app, |s| {
        s.travel.mode = mode;
        s.announce_travel();
    })
}

extern "C" fn on_zoom_in(mut data: RefAny, _info: CallbackInfo) -> Update {
    with_map(&mut data, |s| s.zoom_by(1.0))
}

extern "C" fn on_zoom_out(mut data: RefAny, _info: CallbackInfo) -> Update {
    with_map(&mut data, |s| s.zoom_by(-1.0))
}

extern "C" fn on_locate(mut data: RefAny, info: CallbackInfo) -> Update {
    let fix = info
        .get_location_fix()
        .into_option()
        .map(|f| (f.latitude_deg, f.longitude_deg));
    with_map(&mut data, |s| {
        s.toggle_locate();
        s.last_fix = fix;
        if let (true, Some((lat, lon))) = (s.locating, fix) {
            s.centre_on(lat, lon);
        }
    })
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

/// The sensors and the location every tick; the viewport is kept in
/// settings.json once it has rested `VIEW_SAVE_IDLE_TICKS` ticks.
extern "C" fn tick(mut data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    const LOCATE_TIMEOUT_TICKS: u32 = 200;
    let mag = info
        .callback_info
        .get_sensor_reading(SensorKind::Magnetometer)
        .into_option();
    let fix = info.callback_info.get_location_fix().into_option();
    let mut changed = false;
    let mut keep_view = None;
    if let Some(mut s) = data.downcast_mut::<MapState>() {
        if let Some(r) = mag {
            if s.has_mag {
                s.mag_x = s.mag_x * 0.8 + r.x * 0.2;
                s.mag_y = s.mag_y * 0.8 + r.y * 0.2;
            } else {
                s.mag_x = r.x;
                s.mag_y = r.y;
                s.has_mag = true;
            }
            changed = true;
        }
        if s.locating {
            match fix {
                Some(f) => {
                    let here = (f.latitude_deg, f.longitude_deg);
                    if s.last_fix != Some(here) {
                        s.last_fix = Some(here);
                        s.centre_on(here.0, here.1);
                        changed = true;
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
        Timer::create(app.clone(), tick, info.get_system_time_fn()),
    );
    kit::spawn_file_jobs(&mut info, &root, vec![FileJob::Get { key }], app, TAG_LOAD, on_files_done);
    Update::DoNothing
}

// ==== Entry ====

pub fn start() {
    let args = match AppArgs::from_env(&SPEC) {
        Ok(a) => a,
        Err(why) => {
            eprintln!("{why}");
            std::process::exit(2);
        }
    };
    let kit_ref = kit::create_kit(SPEC, ABOUT, &SHORTCUTS, &[], args);
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
        layer: MapTileLayer::default(),
        locating: false,
        last_fix: None,
        pins: Vec::new(),
        selected: None,
        dismissed_at: None,
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
    };
    let app = App::create(RefAny::new(state), kit::app_config(&kit_ref));
    let window =
        kit::window_options(&kit_ref, layout, (1100.0, 720.0), (640.0, 420.0), on_window_created);
    app.run(window);
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
