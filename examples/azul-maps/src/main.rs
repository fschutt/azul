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
use azul::widgets::{MapTileLayer, MapViewport, MapWidget};

struct MapState {
    viewport: MapViewport,
    /// Layer configuration is stable across the demo's lifetime;
    /// kept in state so the layout callback can rebuild the widget
    /// each frame with the same parameters.
    layer: MapTileLayer,
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
const MAP_CONTAINER: &str = "flex-grow: 1; position: relative; \
    background: #cbd2d8; overflow: hidden;";
const ATTRIB: &str = "position: absolute; right: 6px; bottom: 6px; \
    background: rgba(255,255,255,0.85); padding: 3px 6px; \
    font-size: 10px; color: #444; border-radius: 3px;";

// ───────── Layout ─────────────────────────────────────────────────────

extern "C" fn layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let snapshot: Option<(MapViewport, MapTileLayer)> = data
        .downcast_ref::<MapState>()
        .map(|s| (s.viewport, s.layer.clone()));

    let Some((viewport, layer)) = snapshot else {
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
                            data,
                            on_recentre,
                        ),
                ),
        );

    let map = MapWidget::create(layer)
        .with_viewport(viewport)
        .dom();

    let map_container = Dom::create_div()
        .with_css(MAP_CONTAINER)
        .with_child(map)
        .with_child(
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

fn main() {
    let data = RefAny::new(MapState::new());
    let config = AppConfig::create();
    let app = App::create(data, config);
    let window = WindowCreateOptions::create(layout);
    app.run(window);
}
