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
use azul::dom::GeolocationProbeConfig;
use azul::widgets::{MapTileLayer, MapViewport, MapWidget};

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
}

impl MapState {
    fn new() -> Self {
        Self {
            viewport: MapViewport {
                // Centre on San Francisco. Pick somewhere recognisable
                // so the tile-grid math is easy to eyeball.
                centre_lat_deg: 37.7749,
                centre_lon_deg: -122.4194,
                zoom: 11.0,
                bearing_deg: 0.0,
                pitch_deg: 0.0,
            },
            layer: MapTileLayer::default(),
            locating: false,
            last_fix: None,
            pins: Vec::new(),
            view_px: None,
        }
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
        self.viewport.zoom = 11.0;
    }

    fn toggle_locate(&mut self) {
        self.locating = !self.locating;
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
    padding: 10px 16px; flex-direction: row; align-items: center; \
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
// Transparent full-cover layer that captures map taps (drop-a-pin).
const TAP_OVERLAY: &str = "position: absolute; left: 0px; top: 0px; \
    width: 100%; height: 100%; background: transparent;";

// ───────── Layout ─────────────────────────────────────────────────────

extern "C" fn layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let snapshot: Option<(
        MapViewport,
        MapTileLayer,
        bool,
        Option<(f64, f64)>,
        Vec<(f64, f64)>,
        Option<(f32, f32)>,
    )> = data.downcast_ref::<MapState>().map(|s| {
        (
            s.viewport,
            s.layer.clone(),
            s.locating,
            s.last_fix,
            s.pins.clone(),
            s.view_px,
        )
    });

    let Some((viewport, layer, locating, last_fix, pins, view_px)) = snapshot else {
        return Dom::create_body();
    };

    let attribution_text = layer.attribution.as_str().to_owned();
    let header_text = format!(
        "AzulMaps — centre {:.4}°, {:.4}° · zoom {:.1}",
        viewport.centre_lat_deg, viewport.centre_lon_deg, viewport.zoom
    );

    let header = Dom::create_div()
        .with_css(HEADER)
        .with_child(Dom::create_text(header_text.as_str()))
        .with_child(
            Dom::create_div()
                .with_css("flex-direction: row;")
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
                        } else {
                            "Locate"
                        }))
                        .with_callback(
                            EventFilter::Hover(HoverEventFilter::MouseUp),
                            data.clone(),
                            on_locate,
                        ),
                ),
        );

    let map = MapWidget::create(layer)
        .with_viewport(viewport)
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
            let (px, py) = latlon_to_px(viewport, *lat, *lon, w, h);
            let style = format!(
                "position: absolute; left: {:.1}px; top: {:.1}px; \
                 width: 14px; height: 14px; margin-left: -7px; margin-top: -14px; \
                 background: #d0021b; border-radius: 7px 7px 7px 0px; \
                 transform: rotate(45deg); box-shadow: 0px 1px 2px rgba(0,0,0,0.4);",
                px, py,
            );
            map_container =
                map_container.with_child(Dom::create_div().with_css(style.as_str()));
        }
    }

    map_container = map_container
        .with_child(
            Dom::create_div()
                .with_css(ATTRIB)
                .with_child(Dom::create_text(attribution_text.as_str())),
        )
        // Full-cover transparent layer that captures taps to drop a pin.
        // Last child so it sits on top. The demo pans via the toolbar (not
        // map-drag), so capturing pointer events here is fine.
        .with_child(
            Dom::create_div()
                .with_css(TAP_OVERLAY)
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::MouseUp),
                    data.clone(),
                    on_map_tap,
                )
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::TouchEnd),
                    data,
                    on_map_tap,
                ),
        );

    Dom::create_body()
        .with_css(ROOT)
        .with_child(header)
        .with_child(map_container)
}

// ───────── Callbacks ──────────────────────────────────────────────────

extern "C" fn on_zoom_in(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut s) = data.downcast_mut::<MapState>() {
        s.zoom_in();
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

extern "C" fn on_map_tap(mut data: RefAny, info: CallbackInfo) -> Update {
    // The tap overlay fills the map container, so its hit rect gives the
    // container pixel size; cursor-relative-to-node gives the tap point
    // within it (falling back to viewport-minus-origin).
    let rect = match info.get_hit_node_rect().into_option() {
        Some(r) => r,
        None => return Update::DoNothing,
    };
    let (w, h) = (rect.size.width, rect.size.height);
    if w < 1.0 || h < 1.0 {
        return Update::DoNothing;
    }
    let local = info
        .get_cursor_relative_to_node()
        .into_option()
        .map(|c| (c.x, c.y))
        .or_else(|| {
            info.get_cursor_relative_to_viewport()
                .into_option()
                .map(|p| (p.x - rect.origin.x, p.y - rect.origin.y))
        });
    let (tx, ty) = match local {
        Some(v) => v,
        None => return Update::DoNothing,
    };
    if let Some(mut s) = data.downcast_mut::<MapState>() {
        s.view_px = Some((w, h));
        let (lat, lon) = tap_to_latlon(s.viewport, tx, ty, w, h);
        s.pins.push((lat, lon));
    }
    Update::RefreshDom
}

/// Tap position (px within the container) → `(lat, lon)`. Linear
/// small-angle Mercator — the same approximation the pan handler uses;
/// accurate at city zooms, drifts only for taps far from centre near the
/// poles. Exact inverse of [`latlon_to_px`].
fn tap_to_latlon(vp: MapViewport, tx: f32, ty: f32, w: f32, h: f32) -> (f64, f64) {
    let world = 256.0_f64 * 2.0_f64.powf(vp.zoom as f64);
    let dx = (tx - w * 0.5) as f64;
    let dy = (ty - h * 0.5) as f64;
    let lon = (vp.centre_lon_deg + dx * 360.0 / world).clamp(-180.0, 180.0);
    let cos_lat = vp.centre_lat_deg.to_radians().cos();
    let lat = (vp.centre_lat_deg - dy * 360.0 / world * cos_lat).clamp(-85.0, 85.0);
    (lat, lon)
}

/// `(lat, lon)` → px within the container. Inverse of [`tap_to_latlon`].
fn latlon_to_px(vp: MapViewport, lat: f64, lon: f64, w: f32, h: f32) -> (f32, f32) {
    let world = 256.0_f64 * 2.0_f64.powf(vp.zoom as f64);
    let cos_lat = vp.centre_lat_deg.to_radians().cos();
    let px = w as f64 * 0.5 + (lon - vp.centre_lon_deg) * world / 360.0;
    let py = h as f64 * 0.5 - (lat - vp.centre_lat_deg) * world / (360.0 * cos_lat);
    (px as f32, py as f32)
}

fn main() {
    let data = RefAny::new(MapState::new());
    let config = AppConfig::create();
    let app = App::create(data, config);
    let window = WindowCreateOptions::create(layout);
    app.run(window);
}
