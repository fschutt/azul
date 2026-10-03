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
/// A column that takes the rest of its parent.
const COLUMN: &str = "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;";
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
    /// The pins file's key in the data tree (`maps/pins.json`).
    pins_key: String,
    /// A pins write is in flight; another change waits for it (one writer).
    saving: bool,
    save_pending: bool,
    /// What went wrong with the pins file, shown in the status bar.
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

/// One toolbar button: an icon, its accessible name, its id, its action.
fn tool(app: &RefAny, icon: &str, name: &str, id: AzString, on_click: ButtonOnClickCallbackType) -> Dom {
    Button::create("")
        .with_icon(icon)
        .with_on_click(app.clone(), on_click)
        .dom()
        .with_id(id)
        .with_accessibility_name(name)
}

/// The address-bar slot: pan, zoom, home, locate, clear, the centre, the gear.
fn toolbar(s: &MapState, app: &RefAny) -> Dom {
    let locate_label = if s.locating {
        "Locating\u{2026}"
    } else if s.locate_failed {
        "Location unavailable"
    } else {
        "Locate"
    };
    Dom::create_div()
        .with_css(TOOLBAR)
        .with_child(tool(app, "arrow_back", "Pan west", ids::PAN_LEFT, on_pan_left))
        .with_child(tool(app, "arrow_forward", "Pan east", ids::PAN_RIGHT, on_pan_right))
        .with_child(tool(app, "arrow_upward", "Pan north", ids::PAN_UP, on_pan_up))
        .with_child(tool(app, "arrow_downward", "Pan south", ids::PAN_DOWN, on_pan_down))
        .with_child(tool(app, "remove", "Zoom out", ids::ZOOM_OUT, on_zoom_out))
        .with_child(tool(app, "add", "Zoom in", ids::ZOOM_IN, on_zoom_in))
        .with_child(tool(app, "home", "Back to the start", ids::RECENTRE, on_recentre))
        .with_child(
            Button::create(locate_label)
                .with_icon("my_location")
                .with_toggled(s.locating)
                .with_on_click(app.clone(), on_locate as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::LOCATE),
        )
        .with_child(
            Button::create("Clear pins")
                .with_icon("delete_sweep")
                .with_on_click(app.clone(), on_clear_pins as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::CLEAR_PINS),
        )
        .with_child(
            Dom::create_div()
                .with_css("flex-grow: 1; min-width: 0px; text-align: right; font-size: 12px; white-space: nowrap; overflow: hidden;")
                .with_child(Dom::create_span_with_text(coords(
                    s.viewport.centre_lat_deg,
                    s.viewport.centre_lon_deg,
                ))),
        )
        .with_child(tool(app, "settings", "Settings", ids::SETTINGS, on_settings_open))
}

/// One pin of the pins pane: which pin, and the app.
struct PinRef {
    app: RefAny,
    index: usize,
}

/// The tree slot: every pin, newest last; a click centres the map on it.
fn pins_pane(s: &MapState, app: &RefAny) -> Dom {
    if s.pins.is_empty() {
        return ShellEmptyState::create("No pins yet")
            .with_icon("place")
            .with_detail("Click the map to drop a pin.")
            .dom()
            .with_id(ids::PINS);
    }
    let mut column = Dom::create_div()
        .with_css(PINS_COLUMN)
        .with_id(ids::PINS)
        .with_accessibility_name("Pins");
    for (index, (lat, lon)) in s.pins.iter().enumerate() {
        column.add_child(
            Button::create(coords(*lat, *lon))
                .with_icon("place")
                .with_on_click(
                    RefAny::new(PinRef {
                        app: app.clone(),
                        index,
                    }),
                    on_pin_row as ButtonOnClickCallbackType,
                )
                .dom(),
        );
    }
    column
}

/// The content slot: the map, the pins on it, the location dot, the compass.
fn map_area(s: &MapState, app: &RefAny) -> Dom {
    let map = MapWidget::create(s.layer.clone())
        .with_viewport(s.viewport)
        .with_on_viewport_changed(app.clone(), on_viewport_changed)
        .with_on_pin_tap(app.clone(), on_pin_tap)
        .dom();
    let mut area = Dom::create_div()
        .with_css(MAP_CONTAINER)
        .with_id(ids::MAP)
        .with_child(map);

    if s.locating {
        let readout = match s.last_fix {
            Some((lat, lon)) => format!("You are here: {}", coords(lat, lon)),
            None => "Acquiring location\u{2026}".to_string(),
        };
        area = area
            .with_child(Dom::create_geolocation_probe(GeolocationProbeConfig {
                high_accuracy: true,
                background: false,
                max_accuracy_m: 0.0,
                min_interval_ms: 0,
            }))
            .with_child(Dom::create_div().with_css(LOCATION_DOT))
            .with_child(
                Dom::create_div()
                    .with_css(LOCATION_READOUT)
                    .with_child(Dom::create_span_with_text(readout.as_str())),
            );
    }

    // The pins on the map, once the map's size is known (the first tap).
    if let Some((w, h)) = s.view_px {
        for (lat, lon) in &s.pins {
            let p = MapWidget::px_at_latlon(
                s.viewport,
                MapLatLon {
                    lat_deg: *lat,
                    lon_deg: *lon,
                },
                LogicalSize::create(w, h),
            );
            let marker = format!(
                "position: absolute; left: {:.1}px; top: {:.1}px; width: 14px; height: 14px; \
                 margin-left: -7px; margin-top: -14px; background: #d0021b; border-radius: 7px \
                 7px 7px 0px; transform: rotate(45deg); box-shadow: 0px 1px 2px rgba(0,0,0,0.4);",
                p.x, p.y,
            );
            area = area.with_child(Dom::create_div().with_css(marker.as_str()));
        }
    }

    if let Some(h) = s.heading() {
        let needle = format!(
            "width: 8px; height: 42px; display: flex; flex-direction: column; transform: \
             rotate({:.1}deg);",
            -h,
        );
        area = area.with_child(
            Dom::create_div().with_css(COMPASS_BADGE).with_child(
                Dom::create_div()
                    .with_css(needle.as_str())
                    .with_child(Dom::create_div().with_css(NEEDLE_N))
                    .with_child(Dom::create_div().with_css(NEEDLE_S)),
            ),
        );
    }
    area
}

/// The details slot: where the map looks.
fn details(s: &MapState) -> Dom {
    let mut pane = DetailsPane::create("Map centre")
        .with_icon("map")
        .with_subtitle(coords(s.viewport.centre_lat_deg, s.viewport.centre_lon_deg))
        .with_property("Latitude", format!("{:.5}", s.viewport.centre_lat_deg))
        .with_property("Longitude", format!("{:.5}", s.viewport.centre_lon_deg))
        .with_property("Zoom", format!("{:.1}", s.viewport.zoom))
        .with_property("Pins", s.pins.len().to_string());
    if let Some(h) = s.heading() {
        pane = pane.with_property("Heading", format!("{} {h:03.0}\u{b0}", cardinal(h)));
    }
    pane.dom()
}

/// The status bar: the zoom, the pins, a notice, the tiles' attribution.
fn status_bar(s: &MapState) -> Dom {
    let mut segments = vec![
        StatusBarSegment::create(format!("Zoom {:.1}", s.viewport.zoom)),
        StatusBarSegment::create(match s.pins.len() {
            1 => "1 pin".to_string(),
            n => format!("{n} pins"),
        }),
    ];
    if !s.notice.is_empty() {
        segments.push(StatusBarSegment::create(s.notice.as_str()).with_icon("warning"));
    }
    segments.push(StatusBarSegment::create(s.layer.attribution.as_str()));
    StatusBar::create(segments).dom()
}

extern "C" fn layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    // Reading the mode makes a light / dark switch rebuild the window (the
    // tiles' style follows it).
    let _mode = info.get_mode();
    let app = data.clone();
    let Some(s) = data.downcast_ref::<MapState>() else {
        return Dom::create_body();
    };
    let shell = if kit::settings_open(&s.kit) {
        // azul-appkit's settings page: Appearance, Data, Shortcuts, About.
        Dom::create_div()
            .with_css(COLUMN)
            .with_child(kit::title_row(SPEC.name))
            .with_child(kit::settings_page(&s.kit, Vec::new()))
    } else {
        BrowserShell::create(toolbar(&s, &app), pins_pane(&s, &app), map_area(&s, &app))
            .with_details(details(&s))
            .office_shell()
            .with_title_row(kit::title_row(SPEC.name))
            .with_status_bar(status_bar(&s))
            .dom()
    };
    let column = Dom::create_div().with_css(COLUMN).with_child(shell);
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

extern "C" fn on_pin_tap(mut data: RefAny, mut info: CallbackInfo, coord: MapLatLon) -> Update {
    let size = info.get_hit_node_rect().into_option().map(|r| (r.size.width, r.size.height));
    let update = with_map(&mut data, |s| {
        s.pins.push((coord.lat_deg, coord.lon_deg));
        if size.is_some() {
            s.view_px = size;
        }
    });
    save_pins(&data, &mut info);
    update
}

extern "C" fn on_pin_row(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some((mut app, index)) = data.downcast_ref::<PinRef>().map(|p| (p.app.clone(), p.index)) else {
        return Update::DoNothing;
    };
    with_map(&mut app, |s| {
        if let Some((lat, lon)) = s.pins.get(index).copied() {
            s.centre_on(lat, lon);
        }
    })
}

extern "C" fn on_clear_pins(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let update = with_map(&mut data, |s| s.pins.clear());
    save_pins(&data, &mut info);
    update
}

extern "C" fn on_zoom_in(mut data: RefAny, _info: CallbackInfo) -> Update {
    with_map(&mut data, |s| s.zoom_by(1.0))
}

extern "C" fn on_zoom_out(mut data: RefAny, _info: CallbackInfo) -> Update {
    with_map(&mut data, |s| s.zoom_by(-1.0))
}

extern "C" fn on_recentre(mut data: RefAny, _info: CallbackInfo) -> Update {
    with_map(&mut data, MapState::recentre)
}

extern "C" fn on_pan_left(mut data: RefAny, _info: CallbackInfo) -> Update {
    with_map(&mut data, |s| s.pan(-1.0, 0.0))
}

extern "C" fn on_pan_right(mut data: RefAny, _info: CallbackInfo) -> Update {
    with_map(&mut data, |s| s.pan(1.0, 0.0))
}

extern "C" fn on_pan_up(mut data: RefAny, _info: CallbackInfo) -> Update {
    with_map(&mut data, |s| s.pan(0.0, -1.0))
}

extern "C" fn on_pan_down(mut data: RefAny, _info: CallbackInfo) -> Update {
    with_map(&mut data, |s| s.pan(0.0, 1.0))
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
    let (data_root, pins_key, kept) = {
        let mut k = kit_ref.clone();
        let read = match k.downcast_ref::<kit::Kit>() {
            Some(k) => (
                k.data_root.clone(),
                k.key(PINS_FILE),
                k.settings.get(VIEW_KEY).and_then(|text| parse_view(text)),
            ),
            None => (PathBuf::new(), String::new(), None),
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
        view_px: None,
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
