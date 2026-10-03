//! AzMaps: the `MapWidget` (layout/src/widgets/map.rs) in an app, on the
//! browser shell (S5).
//!
//! ```text
//! ┌ title row ───────────────────────────────────────────────────────────┐
//! │ [←][→][↑][↓] [−][+] [home][locate][clear]   37.7749° N 122.4194° W [⚙]│  address bar
//! ├ pins ──────┬ map ─────────────────────────────────────┬ details ─────┤
//! │ 37.7, -122 │  tiles, pins, the location dot, compass  │ Centre ...   │
//! ├────────────┴──────────────────────────────────────────┴──────────────┤
//! │ centre · zoom · attribution                                     status │
//! └──────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! The pins pane lists every pin (a click centres the map on it); a click
//! on the map drops one. Pins are kept in the data tree (`maps/pins.json`,
//! through azul-appkit's file jobs on a Thread); the last viewport in
//! settings.json, so the map opens where it was left. azul-appkit gives the
//! switches (`--theme`, `--mode`, `--size`, `--shot`, `--data-dir`), the
//! settings page (the gear, Mod+,: Appearance, Data, Shortcuts, About).
//! Arrows pan, `+` / `-` zoom.
//!
//! stdout, for scripts (`scripts/azmaps_e2e.py`): `AZMAPS_VIEW <lat> <lon>
//! <zoom>` on every viewport change, `AZMAPS_PINS <n>` when the pins change,
//! `AZMAPS_PINS_LOADED <n>` / `AZMAPS_PINS_SAVED <n>` / `AZMAPS_PINS_ERROR`.

pub mod ids;
pub mod model;

use std::path::PathBuf;

use azul::{
    callbacks::{ButtonOnClickCallbackType, TimerCallbackInfo, TimerCallbackReturn},
    dom::{GeolocationProbeConfig, VirtualKeyCode},
    prelude::*,
    sensor::SensorKind,
    shells::{BrowserShell, ShellEmptyState, ShellThemeAccent, ShellThemeScope},
    str::String as AzString,
    task::TerminateTimer,
    widgets::{
        Button, DetailsPane, MapLatLon, MapTileLayer, MapViewport, MapWidget, StatusBar,
        StatusBarSegment,
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
    cardinal, pan_tiles, parse_view, pins_from_json, pins_to_json, view_line, view_value, HOME,
    MAX_ZOOM, MIN_ZOOM, PINS_FILE, VIEW_KEY,
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

/// Map ground under the tiles, and the overlays' paint: map content, the
/// same in every theme and mode (the tiles follow the mode themselves).
const MAP_CONTAINER: &str =
    "flex-grow: 1; position: relative; background: #cbd2d8; overflow: hidden; min-height: 0px;";
const COMPASS_BADGE: &str = "position: absolute; right: 12px; top: 12px; width: 56px; height: \
                             56px; border-radius: 28px; background: rgba(20,20,28,0.85); border: \
                             2px solid #6a7080; display: flex; align-items: center; \
                             justify-content: center; box-shadow: 0px 1px 4px rgba(0,0,0,0.4);";
const NEEDLE_N: &str = "flex-grow: 1; background: #e74c3c; border-radius: 4px 4px 0px 0px;";
const NEEDLE_S: &str = "flex-grow: 1; background: #cfd2d8; border-radius: 0px 0px 4px 4px;";
const LOCATION_DOT: &str = "position: absolute; left: 50%; top: 50%; width: 16px; height: 16px; \
                            margin-left: -8px; margin-top: -8px; background: #4285f4; \
                            border-radius: 8px; box-shadow: 0px 0px 0px 3px rgba(66,133,244,0.35);";
const LOCATION_READOUT: &str = "position: absolute; left: 50%; top: 12px; margin-left: -90px; \
                                width: 180px; text-align: center; background: \
                                rgba(66,133,244,0.92); color: white; padding: 4px 8px; \
                                border-radius: 4px; font-size: 12px;";
/// The toolbar row (the shell's address-bar slot).
const TOOLBAR: &str = "display: flex; flex-direction: row; align-items: center; gap: 4px; \
                       padding: 4px 8px; flex-grow: 1; min-width: 0px;";
/// The pins pane's column.
const PINS_COLUMN: &str = "display: flex; flex-direction: column; gap: 2px; padding: 6px; \
                           overflow-y: auto; flex-grow: 1; min-height: 0px;";

// ==== State ====

struct MapState {
    viewport: MapViewport,
    layer: MapTileLayer,
    locating: bool,
    last_fix: Option<(f64, f64)>,
    pins: Vec<(f64, f64)>,
    view_px: Option<(f32, f32)>,
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

    fn recentre(&mut self) {
        let (lat, lon, zoom) = HOME;
        self.viewport.zoom = zoom;
        self.centre_on(lat, lon);
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
}

// ==== LAYOUT (next commit) ====
