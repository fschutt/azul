//! AzulMaps — the P3 goal app from SUPER_PLAN_2.
//!
//! Exercises the `MapWidget` skeleton landed in
//! `layout/src/widgets/map.rs`. The widget computes the visible-tile
//! XYZ grid via Web Mercator math, builds one GPU-translated `<div>`
//! per tile, and holds a `MapTileCache` `RefAny` dataset that
//! survives relayout via a `DatasetMergeCallback`. Real MVT decode +
//! HTTP fetch land in follow-up ticks; this demo wires the grid +
//! viewport bookkeeping + a simple toolbar (zoom in / zoom out / recentre).
//!
//! Pan via touch / drag will land once the `GestureAndDragManager` is
//! wired through the widget — for now the demo lets you nudge the
//! viewport via on-screen buttons so the grid recompute is visible.
//!
//! Compose a `Dom::create_geolocation_probe(...)` anywhere in the
//! subtree to opt into "this app needs GPS" — the widget itself is
//! agnostic of location; the framework's permission-as-DOM plumbing
//! routes the prompt automatically (per P3.1).

use azul::prelude::*;
use azul::dom::{GeolocationProbeConfig, MapPinTapCallback};
use azul::widgets::MapViewportChangedCallback;
use azul::misc::SensorKind;
use azul::option::OptionRefAny;
use azul::task::TerminateTimer;
use azul::widgets::{MapLatLon, MapTileLayer, MapViewport, MapWidget};

struct MapState {
    viewport: MapViewport,
    /// Layer configuration is stable across the demo's lifetime;
    /// kept in state so the layout callback can rebuild the widget
    /// each frame with the same parameters.
    layer: MapTileLayer,
    /// When `true`, the layout composes an invisible
    /// `Dom::create_geolocation_probe(...)` into the map subtree. The
    /// framework's permission-as-DOM diff then requests location and
    /// (once a platform backend delivers a fix) the "you are here" dot
    /// can be placed. Toggled by the "Locate" button.
    locating: bool,
    /// Last geolocation fix `(lat, lon)` read from `CallbackInfo::
    /// get_location_fix()`, captured on the Locate toggle. `None` until a
    /// backend delivers one. (Refreshes on toggle; a live readout would
    /// poll via a Timer — out of scope for the demo.)
    last_fix: Option<(f64, f64)>,
    /// Pins dropped by tapping the map, stored as `(lat, lon)` so they
    /// track the viewport across pan/zoom (re-projected each layout).
    pins: Vec<(f64, f64)>,
    /// Map container pixel size, cached from the tap callback (layout()
    /// can't measure it). Used to project pins lat/lon → screen px. `None`
    /// until the first tap.
    view_px: Option<(f32, f32)>,
    /// Smoothed horizontal magnetometer vector (µT) for the compass rose,
    /// kept live by a Timer reading `get_sensor_reading(Magnetometer)`. The
    /// vector (not the angle) is low-pass-filtered so smoothing doesn't
    /// break across the 0°/360° wrap. `has_mag` is `false` until the first
    /// sample, which keeps the rose hidden where there's no magnetometer.
    mag_x: f32,
    mag_y: f32,
    has_mag: bool,
    /// `true` after a Locate attempt timed out with no fix (geolocation
    /// unavailable on this system) — surfaced in the button so "Locating…"
    /// can't hang forever (bug #8). Cleared when Locate is re-enabled.
    locate_failed: bool,
    /// Frames elapsed since Locate was enabled without a fix yet; once it
    /// passes `LOCATE_TIMEOUT_TICKS` the attempt fails. Reset on each toggle.
    locate_ticks: u32,
}

impl MapState {
    fn new() -> Self {
        Self {
            viewport: MapViewport {
                // Centre on San Francisco. Pick somewhere recognisable
                // so the tile-grid math is easy to eyeball. Zoom 2 keeps us in
                // the MapLibre demo-tiles' coverage (z0–6, the no-API-key default).
                centre_lat_deg: 37.7749,
                centre_lon_deg: -122.4194,
                zoom: 2.0,
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
        }
    }

    /// Compass heading in degrees [0, 360), or `None` until a magnetometer
    /// sample arrives. Simplified (assumes the device is held flat — no
    /// tilt compensation, no declination correction), which is plenty to
    /// demonstrate the live magnetometer; a true heading would fuse the
    /// accelerometer + local declination.
    fn heading(&self) -> Option<f32> {
        if !self.has_mag {
            return None;
        }
        Some((self.mag_y.atan2(self.mag_x).to_degrees() + 360.0) % 360.0)
    }

    fn zoom_in(&mut self) {
        self.viewport.zoom =
            (self.viewport.zoom + 1.0).min(self.layer.max_zoom as f32);
    }

    fn zoom_out(&mut self) {
        self.viewport.zoom =
            (self.viewport.zoom - 1.0).max(self.layer.min_zoom as f32);
    }

    /// Recentre the demo on its starting point.
    fn recentre(&mut self) {
        self.viewport.centre_lat_deg = 37.7749;
        self.viewport.centre_lon_deg = -122.4194;
        self.viewport.zoom = 2.0;
    }

    fn toggle_locate(&mut self) {
        self.locating = !self.locating;
        // Fresh attempt: clear the failure flag + restart the timeout clock.
        if self.locating {
            self.locate_failed = false;
            self.locate_ticks = 0;
        }
    }

    /// Nudge the viewport ~half a tile in tile-space at the current
    /// integer zoom. Hooks up the four arrow buttons to the same
    /// Web-Mercator math the widget uses internally; useful until
    /// the gesture wiring lands.
    fn pan(&mut self, dx: f64, dy: f64) {
        let z_int = self.viewport.zoom.floor() as i32;
        let tile_count = (1u32 << z_int.max(0) as u32) as f64;
        // tile-x is `(lon + 180)/360 * 2^z`; invert by stepping a
        // half-tile in tile-x:
        let delta_lon = (dx / 2.0) * (360.0 / tile_count);
        let new_lon = self.viewport.centre_lon_deg + delta_lon;
        // Wrap lon into [-180, 180].
        let wrapped_lon = ((new_lon + 540.0) % 360.0) - 180.0;
        self.viewport.centre_lon_deg = wrapped_lon;

        // Lat is non-linear in Mercator; step in degrees directly
        // (small steps, fine for this demo).
        let delta_lat = (dy / 2.0) * (180.0 / tile_count);
        self.viewport.centre_lat_deg =
            (self.viewport.centre_lat_deg + delta_lat).clamp(-85.0, 85.0);
    }
}

// ───────── Styles ─────────────────────────────────────────────────────

const ROOT: &str = "display: flex; flex-direction: column; height: 100%;";
const HEADER: &str = "background: #2b2b2b; color: white; \
    display: flex; padding: 10px 16px; flex-direction: row; align-items: center; \
    justify-content: space-between; font-family: sans-serif; \
    font-size: 14px; flex-shrink: 0;";
const BTN: &str = "background: #4a90e2; color: white; \
    padding: 6px 12px; border-radius: 4px; cursor: pointer; \
    margin-left: 6px; font-size: 13px;";
const BTN_ON: &str = "background: #d0021b; color: white; \
    padding: 6px 12px; border-radius: 4px; cursor: pointer; \
    margin-left: 6px; font-size: 13px;";
const MAP_CONTAINER: &str = "flex-grow: 1; position: relative; \
    background: #cbd2d8; overflow: hidden;";
// Compass rose badge (top-right) + its two-tone needle (red = north).
const COMPASS_BADGE: &str = "position: absolute; right: 12px; top: 12px; \
    width: 56px; height: 56px; border-radius: 28px; \
    background: rgba(20,20,28,0.85); border: 2px solid #6a7080; \
    display: flex; align-items: center; justify-content: center; \
    box-shadow: 0px 1px 4px rgba(0,0,0,0.4);";
const NEEDLE_N: &str = "flex-grow: 1; background: #e74c3c; \
    border-radius: 4px 4px 0px 0px;";
const NEEDLE_S: &str = "flex-grow: 1; background: #cfd2d8; \
    border-radius: 0px 0px 4px 4px;";
const ATTRIB: &str = "position: absolute; right: 6px; bottom: 6px; \
    background: rgba(255,255,255,0.85); padding: 3px 6px; \
    font-size: 10px; color: #444; border-radius: 3px;";
// "You are here" marker at the map centre. `on_locate` recentres the
// viewport on the fix, so the centre dot marks the user's position
// without needing a per-pixel projection of lat/lon to the container.
const LOCATION_DOT: &str = "position: absolute; left: 50%; top: 50%; \
    width: 16px; height: 16px; margin-left: -8px; margin-top: -8px; \
    background: #4285f4; border-radius: 8px; \
    box-shadow: 0px 0px 0px 3px rgba(66,133,244,0.35);";
// Coordinate read-out for the live fix, top-centre over the map.
const LOCATION_READOUT: &str = "position: absolute; left: 50%; top: 12px; \
    margin-left: -90px; width: 180px; text-align: center; \
    background: rgba(66,133,244,0.92); color: white; padding: 4px 8px; \
    border-radius: 4px; font-size: 12px; font-family: sans-serif;";

// ───────── Layout ─────────────────────────────────────────────────────

extern "C" fn layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let snapshot: Option<(
        MapViewport,
        MapTileLayer,
        bool,
        Option<(f64, f64)>,
        Vec<(f64, f64)>,
        Option<(f32, f32)>,
        bool,
    )> = data.downcast_ref::<MapState>().map(|s| {
        (
            s.viewport,
            s.layer.clone(),
            s.locating,
            s.last_fix,
            s.pins.clone(),
            s.view_px,
            s.locate_failed,
        )
    });

    let Some((viewport, layer, locating, last_fix, pins, view_px, locate_failed)) = snapshot else {
        return Dom::create_body();
    };

    // Live compass heading (P6 magnetometer): `None` until a sample arrives,
    // so the rose only appears where there's a magnetometer. Read separately
    // (a second shared borrow) to keep the snapshot tuple untouched.
    let heading = data.downcast_ref::<MapState>().and_then(|s| s.heading());

    let attribution_text = layer.attribution.as_str().to_owned();
    let mut header_text = format!(
        "AzulMaps — centre {:.4}°, {:.4}° · zoom {:.1}",
        viewport.centre_lat_deg, viewport.centre_lon_deg, viewport.zoom
    );
    if let Some(h) = heading {
        header_text.push_str(&format!(" · {} {:03.0}°", cardinal(h), h));
    }

    let header = Dom::create_div()
        .with_css(HEADER)
        .with_child(Dom::create_text(header_text.as_str()))
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row;")
                .with_child(
                    Dom::create_div()
                        .with_css(BTN)
                        .with_child(Dom::create_text("←"))
                        .with_callback(
                            EventFilter::Hover(HoverEventFilter::MouseUp),
                            data.clone(),
                            on_pan_left,
                        ),
                )
                .with_child(
                    Dom::create_div()
                        .with_css(BTN)
                        .with_child(Dom::create_text("→"))
                        .with_callback(
                            EventFilter::Hover(HoverEventFilter::MouseUp),
                            data.clone(),
                            on_pan_right,
                        ),
                )
                .with_child(
                    Dom::create_div()
                        .with_css(BTN)
                        .with_child(Dom::create_text("↑"))
                        .with_callback(
                            EventFilter::Hover(HoverEventFilter::MouseUp),
                            data.clone(),
                            on_pan_up,
                        ),
                )
                .with_child(
                    Dom::create_div()
                        .with_css(BTN)
                        .with_child(Dom::create_text("↓"))
                        .with_callback(
                            EventFilter::Hover(HoverEventFilter::MouseUp),
                            data.clone(),
                            on_pan_down,
                        ),
                )
                .with_child(
                    Dom::create_div()
                        .with_css(BTN)
                        .with_child(Dom::create_text("+"))
                        .with_callback(
                            EventFilter::Hover(HoverEventFilter::MouseUp),
                            data.clone(),
                            on_zoom_in,
                        ),
                )
                .with_child(
                    Dom::create_div()
                        .with_css(BTN)
                        .with_child(Dom::create_text("−"))
                        .with_callback(
                            EventFilter::Hover(HoverEventFilter::MouseUp),
                            data.clone(),
                            on_zoom_out,
                        ),
                )
                .with_child(
                    Dom::create_div()
                        .with_css(BTN)
                        .with_child(Dom::create_text("Recentre"))
                        .with_callback(
                            EventFilter::Hover(HoverEventFilter::MouseUp),
                            data.clone(),
                            on_recentre,
                        ),
                )
                .with_child(
                    Dom::create_div()
                        .with_css(if locating { BTN_ON } else { BTN })
                        .with_child(Dom::create_text(if locating {
                            "Locating…"
                        } else if locate_failed {
                            "Location N/A"
                        } else {
                            "Locate"
                        }))
                        .with_callback(
                            EventFilter::Hover(HoverEventFilter::MouseUp),
                            data.clone(),
                            on_locate,
                        ),
                )
                .with_child(
                    Dom::create_div()
                        .with_css(BTN)
                        .with_child(Dom::create_text("Clear pins"))
                        .with_callback(
                            EventFilter::Hover(HoverEventFilter::MouseUp),
                            data.clone(),
                            on_clear_pins,
                        ),
                ),
        );

    let map = MapWidget::create(layer)
        .with_viewport(viewport)
        // Keep MapState.viewport in sync with widget-internal drags/wheel-zooms.
        // Without this the app state goes stale, and any RefreshDom (the +/−
        // buttons, Recentre) would rebuild the widget with the OLD viewport —
        // snapping the map back. Also live-updates the header readout.
        .with_on_viewport_changed(
            data.clone(),
            MapViewportChangedCallback {
                cb: on_viewport_changed,
                callable: OptionRefAny::None,
            },
        )
        .with_on_pin_tap(
            data.clone(),
            MapPinTapCallback {
                cb: on_pin_tap,
                callable: OptionRefAny::None,
            },
        )
        // `.dom()` wires the built-in tile-fetch worker internally: it HTTP-GETs
        // each visible tile's MVT (.pbf), decodes it, renders it to SVG, and writes
        // the SVG back into the MapTileCache dataset (which the VirtualView then
        // draws). The fetch starts on mount and is freed when the widget unmounts.
        .dom();

    let mut map_container = Dom::create_div()
        .with_css(MAP_CONTAINER)
        .with_child(map);

    // When "Locate" is on, drop an invisible geolocation probe into the
    // map subtree. The framework treats the probe as a permission-as-DOM
    // request: mounting it asks the platform for a location fix. Until a
    // backend delivers one we just draw a placeholder dot at centre so
    // the composition is visible in the demo.
    if locating {
        // Read-back of the live fix (P3.1 `get_location_fix`): show the
        // coordinates once a backend has delivered one, else "acquiring".
        let readout = match last_fix {
            Some((lat, lon)) => format!("You are here: {:.4}, {:.4}", lat, lon),
            None => "Acquiring location…".to_string(),
        };
        map_container = map_container
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
                    .with_child(Dom::create_text(readout.as_str())),
            );
    }

    // Tapped pins, projected lat/lon → screen px via the cached container
    // size (set by the tap callback). They track the viewport: pan/zoom
    // re-projects them each layout.
    if let Some((w, h)) = view_px {
        for (lat, lon) in &pins {
            let p = MapWidget::px_at_latlon(
                viewport,
                MapLatLon {
                    lat_deg: *lat,
                    lon_deg: *lon,
                },
                LogicalSize::create(w, h),
            );
            let (px, py) = (p.x, p.y);
            let style = format!(
                "position: absolute; left: {:.1}px; top: {:.1}px; \
                 width: 14px; height: 14px; margin-left: -7px; margin-top: -14px; \
                 background: #d0021b; border-radius: 7px 7px 7px 0px; \
                 transform: rotate(45deg); box-shadow: 0px 1px 2px rgba(0,0,0,0.4);",
                px, py,
            );
            map_container =
                map_container.with_child(Dom::create_div().with_css(style.as_str()));
            // Callout: the pinned point's coordinates, beside the marker.
            let callout_style = format!(
                "position: absolute; left: {:.1}px; top: {:.1}px; \
                 background: rgba(255,255,255,0.95); color: #222; \
                 padding: 2px 6px; border-radius: 4px; font-size: 11px; \
                 font-family: sans-serif; white-space: nowrap; \
                 box-shadow: 0px 1px 2px rgba(0,0,0,0.3);",
                px + 10.0,
                py - 30.0,
            );
            map_container = map_container.with_child(
                Dom::create_div()
                    .with_css(callout_style.as_str())
                    .with_child(Dom::create_text(
                        format!("{:.4}, {:.4}", lat, lon).as_str(),
                    )),
            );
        }
    }

    // Compass rose (P6 magnetometer): a corner badge whose needle rotates by
    // -heading, so the red north tip keeps pointing at magnetic north as the
    // device turns. Added before the tap overlay so taps still drop pins
    // through the (non-interactive) badge.
    if let Some(h) = heading {
        let needle = format!(
            "width: 8px; height: 42px; display: flex; flex-direction: column; \
             transform: rotate({:.1}deg);",
            -h,
        );
        map_container = map_container.with_child(
            Dom::create_div().with_css(COMPASS_BADGE).with_child(
                Dom::create_div()
                    .with_css(needle.as_str())
                    .with_child(Dom::create_div().with_css(NEEDLE_N))
                    .with_child(Dom::create_div().with_css(NEEDLE_S)),
            ),
        );
    }

    // The MapWidget handles taps itself (via with_on_pin_tap) + pan/pinch via
    // its own pointer handlers, so no tap overlay is needed.
    map_container = map_container.with_child(
        Dom::create_div()
            .with_css(ATTRIB)
            .with_child(Dom::create_text(attribution_text.as_str())),
    );

    Dom::create_body()
        .with_css(ROOT)
        .with_child(header)
        .with_child(map_container)
}

// ───────── Callbacks ──────────────────────────────────────────────────

extern "C" fn on_zoom_in(mut data: RefAny, _info: CallbackInfo) -> Update {
    if std::env::var("AZ_MAP_DEBUG").is_ok() {
        eprintln!("[map-demo] on_zoom_in FIRED");
    }
    if let Some(mut s) = data.downcast_mut::<MapState>() {
        s.zoom_in();
    }
    Update::RefreshDom
}

// Widget-internal pan/zoom (drag, wheel, pinch) → mirror into MapState so the
// next RefreshDom rebuild passes the CURRENT viewport back to the widget (and
// the header readout stays live).
extern "C" fn on_viewport_changed(mut data: RefAny, _info: CallbackInfo, vp: MapViewport) -> Update {
    if let Some(mut s) = data.downcast_mut::<MapState>() {
        s.viewport = vp;
    }
    Update::RefreshDom
}

extern "C" fn on_zoom_out(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut s) = data.downcast_mut::<MapState>() {
        s.zoom_out();
    }
    Update::RefreshDom
}

extern "C" fn on_recentre(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut s) = data.downcast_mut::<MapState>() {
        s.recentre();
    }
    Update::RefreshDom
}

extern "C" fn on_locate(mut data: RefAny, info: CallbackInfo) -> Update {
    // Read the latest fix the geolocation backend delivered (via the
    // public CallbackInfo accessor that P3.1 exposed). `None` until a
    // backend has reported one.
    let fix = info
        .get_location_fix()
        .into_option()
        .map(|f| (f.latitude_deg, f.longitude_deg));
    if let Some(mut s) = data.downcast_mut::<MapState>() {
        s.toggle_locate();
        s.last_fix = fix;
        // Standard "locate me": when enabling Locate with a fix in hand,
        // recentre the viewport on it so the centre dot marks the user's
        // position (no per-pixel projection needed). The fix arrives async,
        // so on a cold first toggle there's none yet — toggling again once
        // a backend has reported recentres; a Timer-driven live recentre is
        // the follow-up.
        if s.locating {
            if let Some((lat, lon)) = fix {
                s.viewport.centre_lat_deg = lat;
                s.viewport.centre_lon_deg = lon;
            }
        }
    }
    Update::RefreshDom
}

extern "C" fn on_clear_pins(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut s) = data.downcast_mut::<MapState>() {
        s.pins.clear();
    }
    Update::RefreshDom
}

extern "C" fn on_pan_left(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut s) = data.downcast_mut::<MapState>() {
        s.pan(-1.0, 0.0);
    }
    Update::RefreshDom
}

extern "C" fn on_pan_right(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut s) = data.downcast_mut::<MapState>() {
        s.pan(1.0, 0.0);
    }
    Update::RefreshDom
}

extern "C" fn on_pan_up(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut s) = data.downcast_mut::<MapState>() {
        s.pan(0.0, -1.0);
    }
    Update::RefreshDom
}

extern "C" fn on_pan_down(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut s) = data.downcast_mut::<MapState>() {
        s.pan(0.0, 1.0);
    }
    Update::RefreshDom
}

/// The map's `on_pin_tap` hook: the widget already detected the tap (no drag)
/// and projected it, so we just record the lat/lon. We also cache the
/// container size (from the hit-node rect) so the pin overlay re-projects on
/// pan/zoom via `MapWidget::px_at_latlon`.
extern "C" fn on_pin_tap(mut data: RefAny, info: CallbackInfo, coord: MapLatLon) -> Update {
    if let Some(mut s) = data.downcast_mut::<MapState>() {
        s.pins.push((coord.lat_deg, coord.lon_deg));
        if let Some(rect) = info.get_hit_node_rect().into_option() {
            s.view_px = Some((rect.size.width, rect.size.height));
        }
    }
    Update::RefreshDom
}

// (Projection now lives in the widget: `MapWidget::latlon_at_px` /
// `px_at_latlon`. The demo's duplicated `tap_to_latlon` / `latlon_to_px` are
// gone — `on_pin_tap` receives the lat/lon, pin rendering uses `px_at_latlon`.)

/// Eight-point cardinal label for a heading in degrees.
fn cardinal(deg: f32) -> &'static str {
    const DIRS: [&str; 8] = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
    DIRS[((((deg + 22.5) % 360.0) / 45.0) as usize) % 8]
}

/// Timer tick: pull the latest magnetometer sample through the
/// `TimerCallbackInfo`'s wrapped `CallbackInfo` and low-pass-filter the
/// horizontal vector (the vector, not the angle, so smoothing survives the
/// 0°/360° wrap), then relayout so the rose follows.
extern "C" fn compass_tick(mut data: RefAny, info: TimerCallbackInfo) -> TimerCallbackReturn {
    /// ~frames a fix-less Locate attempt waits before it's declared failed (#8).
    const LOCATE_TIMEOUT_TICKS: u32 = 200;
    let mag = info
        .callback_info
        .get_sensor_reading(SensorKind::Magnetometer)
        .into_option();
    let fix = info.callback_info.get_location_fix().into_option();
    let mut changed = false;
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
        // Locate: live-recentre on a fix, or time out with feedback so
        // "Locating…" can't hang forever when geolocation is unavailable (#8).
        if s.locating {
            match fix {
                Some(f) => {
                    s.viewport.centre_lat_deg = f.latitude_deg;
                    s.viewport.centre_lon_deg = f.longitude_deg;
                    s.last_fix = Some((f.latitude_deg, f.longitude_deg));
                    s.locate_ticks = 0;
                    changed = true;
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

/// Window-create callback: install the per-frame Timer that keeps the
/// compass live. (The magnetometer is read from a `CallbackInfo`, not the
/// layout callback, so the Timer is what makes the rose turn.)
extern "C" fn startup(data: RefAny, mut info: CallbackInfo) -> Update {
    info.add_timer(
        TimerId::unique(),
        Timer::create(
            data.clone(),
            TimerCallback {
                cb: compass_tick,
                ctx: OptionRefAny::None,
            },
            info.get_system_time_fn(),
        ),
    );
    Update::DoNothing
}

/// Start the app. On desktop/iOS this blocks (iOS via UIApplicationMain inside
/// `App::run`); on Android `App::run` only stashes the window options for
/// libazul's `android_main` to pick up, then returns.
pub fn start() {
    let data = RefAny::new(MapState::new());
    let config = AppConfig::create();
    let app = App::create(data, config);
    let mut window = WindowCreateOptions::create(layout);
    window.create_callback = Some(Callback::create(startup)).into();
    app.run(window);
}

// Android has no `main()`: the OS loads this cdylib and calls libazul's
// `android_main` (via the android-activity glue). `android_main` reads the
// window options that `App::run` stashed, so `start()` must run BEFORE
// `ANativeActivity_onCreate` — i.e. from a library constructor that fires at
// `dlopen` / `System.loadLibrary` time. See guide/mobile.md.
#[cfg(target_os = "android")]
#[ctor::ctor]
fn azul_android_init() {
    start();
}
