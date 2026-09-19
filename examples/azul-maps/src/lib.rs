use azul::{
    dom::GeolocationProbeConfig,
    prelude::*,
    sensor::SensorKind,
    task::TerminateTimer,
    widgets::{MapLatLon, MapTileLayer, MapViewport, MapWidget},
};

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
}

impl MapState {
    fn new() -> Self {
        Self {
            viewport: MapViewport {
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

    fn heading(&self) -> Option<f32> {
        if !self.has_mag {
            return None;
        }
        Some((self.mag_y.atan2(self.mag_x).to_degrees() + 360.0) % 360.0)
    }

    fn zoom_in(&mut self) {
        self.viewport.zoom = (self.viewport.zoom + 1.0).min(self.layer.max_zoom as f32);
    }

    fn zoom_out(&mut self) {
        self.viewport.zoom = (self.viewport.zoom - 1.0).max(self.layer.min_zoom as f32);
    }

    fn recentre(&mut self) {
        self.viewport.centre_lat_deg = 37.7749;
        self.viewport.centre_lon_deg = -122.4194;
        self.viewport.zoom = 2.0;
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
        self.viewport.centre_lon_deg = lon;
        self.viewport.centre_lat_deg = lat;
    }
}

fn pan_tiles(
    lon_deg: f64,
    lat_deg: f64,
    tile_count: f64,
    dx_tiles: f64,
    dy_tiles: f64,
) -> (f64, f64) {
    use std::f64::consts::PI;
    let x = (lon_deg + 180.0) / 360.0 * tile_count + dx_tiles;
    let lon = ((x / tile_count * 360.0 - 180.0) + 540.0).rem_euclid(360.0) - 180.0;
    let lat_rad = lat_deg.to_radians();
    let y = (1.0 - (lat_rad.tan() + 1.0 / lat_rad.cos()).ln() / PI) / 2.0 * tile_count;
    let y = (y + dy_tiles).clamp(0.0, tile_count);
    let lat = (PI * (1.0 - 2.0 * y / tile_count))
        .sinh()
        .atan()
        .to_degrees();
    (lon, lat.clamp(-85.0, 85.0))
}

#[cfg(test)]
mod pan_tests {
    use super::pan_tiles;

    #[test]
    fn up_goes_north_in_both_hemispheres() {
        for lat in [37.7749, -33.8688, 0.0] {
            let (_, north) = pan_tiles(0.0, lat, 4.0, 0.0, -0.5);
            let (_, south) = pan_tiles(0.0, lat, 4.0, 0.0, 0.5);
            assert!(north > lat, "↑ must go north: {lat} → {north}");
            assert!(south < lat, "↓ must go south: {lat} → {south}");
        }
    }

    #[test]
    fn steps_are_exact_in_tile_space_and_east_is_positive() {
        let (lon, lat) = pan_tiles(0.0, 0.0, 4.0, 0.5, 0.0);
        assert!((lon - 45.0).abs() < 1e-9, "{lon}");
        assert!(lat.abs() < 1e-9, "{lat}");
        let (_, lat) = pan_tiles(0.0, 0.0, 2.0, 0.0, 1.0);
        assert!((lat - -85.0).abs() < 1e-9, "{lat}");
        let (lon, _) = pan_tiles(179.0, 0.0, 4.0, 0.5, 0.0);
        assert!((lon - -136.0).abs() < 1e-9, "{lon}");
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

const ROOT: &str = "display: flex; flex-direction: column; height: 100%;";
const HEADER: &str = "background: #2b2b2b; color: white; display: flex; padding: 10px 16px; \
                      flex-direction: row; align-items: center; justify-content: space-between; \
                      font-family: sans-serif; font-size: 14px; flex-shrink: 0;";
const BTN: &str = "background: #4a90e2; color: white; padding: 6px 12px; border-radius: 4px; \
                   cursor: pointer; margin-left: 6px; font-size: 13px;";
const BTN_ON: &str = "background: #d0021b; color: white; padding: 6px 12px; border-radius: 4px; \
                      cursor: pointer; margin-left: 6px; font-size: 13px;";
const MAP_CONTAINER: &str =
    "flex-grow: 1; position: relative; background: #cbd2d8; overflow: hidden;";
const COMPASS_BADGE: &str = "position: absolute; right: 12px; top: 12px; width: 56px; height: \
                             56px; border-radius: 28px; background: rgba(20,20,28,0.85); border: \
                             2px solid #6a7080; display: flex; align-items: center; \
                             justify-content: center; box-shadow: 0px 1px 4px rgba(0,0,0,0.4);";
const NEEDLE_N: &str = "flex-grow: 1; background: #e74c3c; border-radius: 4px 4px 0px 0px;";
const NEEDLE_S: &str = "flex-grow: 1; background: #cfd2d8; border-radius: 0px 0px 4px 4px;";
const ATTRIB: &str = "position: absolute; right: 6px; bottom: 6px; background: \
                      rgba(255,255,255,0.85); padding: 3px 6px; font-size: 10px; color: #444; \
                      border-radius: 3px;";
const LOCATION_DOT: &str = "position: absolute; left: 50%; top: 50%; width: 16px; height: 16px; \
                            margin-left: -8px; margin-top: -8px; background: #4285f4; \
                            border-radius: 8px; box-shadow: 0px 0px 0px 3px rgba(66,133,244,0.35);";
const LOCATION_READOUT: &str = "position: absolute; left: 50%; top: 12px; margin-left: -90px; \
                                width: 180px; text-align: center; background: \
                                rgba(66,133,244,0.92); color: white; padding: 4px 8px; \
                                border-radius: 4px; font-size: 12px; font-family: sans-serif;";

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

    let heading = data.downcast_ref::<MapState>().and_then(|s| s.heading());

    let attribution_text = layer.attribution.as_str().to_owned();
    let mut header_text = format!(
        "AzMaps — centre {:.4}°, {:.4}° · zoom {:.1}",
        viewport.centre_lat_deg, viewport.centre_lon_deg, viewport.zoom
    );
    if let Some(h) = heading {
        header_text.push_str(&format!(" · {} {:03.0}°", cardinal(h), h));
    }

    let header = Dom::create_div()
        .with_css(HEADER)
        .with_child(Dom::create_span_with_text(header_text.as_str()))
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row;")
                .with_child(
                    Dom::create_div()
                        .with_css(BTN)
                        .with_child(Dom::create_span_with_text("←"))
                        .with_callback(
                            EventFilter::Hover(HoverEventFilter::MouseUp),
                            data.clone(),
                            on_pan_left,
                        ),
                )
                .with_child(
                    Dom::create_div()
                        .with_css(BTN)
                        .with_child(Dom::create_span_with_text("→"))
                        .with_callback(
                            EventFilter::Hover(HoverEventFilter::MouseUp),
                            data.clone(),
                            on_pan_right,
                        ),
                )
                .with_child(
                    Dom::create_div()
                        .with_css(BTN)
                        .with_child(Dom::create_span_with_text("↑"))
                        .with_callback(
                            EventFilter::Hover(HoverEventFilter::MouseUp),
                            data.clone(),
                            on_pan_up,
                        ),
                )
                .with_child(
                    Dom::create_div()
                        .with_css(BTN)
                        .with_child(Dom::create_span_with_text("↓"))
                        .with_callback(
                            EventFilter::Hover(HoverEventFilter::MouseUp),
                            data.clone(),
                            on_pan_down,
                        ),
                )
                .with_child(
                    Dom::create_div()
                        .with_css(BTN)
                        .with_child(Dom::create_span_with_text("+"))
                        .with_callback(
                            EventFilter::Hover(HoverEventFilter::MouseUp),
                            data.clone(),
                            on_zoom_in,
                        ),
                )
                .with_child(
                    Dom::create_div()
                        .with_css(BTN)
                        .with_child(Dom::create_span_with_text("−"))
                        .with_callback(
                            EventFilter::Hover(HoverEventFilter::MouseUp),
                            data.clone(),
                            on_zoom_out,
                        ),
                )
                .with_child(
                    Dom::create_div()
                        .with_css(BTN)
                        .with_child(Dom::create_span_with_text("Recentre"))
                        .with_callback(
                            EventFilter::Hover(HoverEventFilter::MouseUp),
                            data.clone(),
                            on_recentre,
                        ),
                )
                .with_child(
                    Dom::create_div()
                        .with_css(if locating { BTN_ON } else { BTN })
                        .with_child(Dom::create_span_with_text(if locating {
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
                        .with_child(Dom::create_span_with_text("Clear pins"))
                        .with_callback(
                            EventFilter::Hover(HoverEventFilter::MouseUp),
                            data.clone(),
                            on_clear_pins,
                        ),
                ),
        );

    let map = MapWidget::create(layer)
        .with_viewport(viewport)
        .with_on_viewport_changed(
            data.clone(),
            on_viewport_changed,
        )
        .with_on_pin_tap(
            data.clone(),
            on_pin_tap,
        )
        .dom();

    let mut map_container = Dom::create_div().with_css(MAP_CONTAINER).with_child(map);

    if locating {
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
                    .with_child(Dom::create_span_with_text(readout.as_str())),
            );
    }

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
                "position: absolute; left: {:.1}px; top: {:.1}px; width: 14px; height: 14px; \
                 margin-left: -7px; margin-top: -14px; background: #d0021b; border-radius: 7px \
                 7px 7px 0px; transform: rotate(45deg); box-shadow: 0px 1px 2px rgba(0,0,0,0.4);",
                px, py,
            );
            map_container = map_container.with_child(Dom::create_div().with_css(style.as_str()));
            let callout_style = format!(
                "position: absolute; left: {:.1}px; top: {:.1}px; background: \
                 rgba(255,255,255,0.95); color: #222; padding: 2px 6px; border-radius: 4px; \
                 font-size: 11px; font-family: sans-serif; white-space: nowrap; box-shadow: 0px \
                 1px 2px rgba(0,0,0,0.3);",
                px + 10.0,
                py - 30.0,
            );
            map_container = map_container.with_child(
                Dom::create_div()
                    .with_css(callout_style.as_str())
                    .with_child(Dom::create_span_with_text(
                        format!("{:.4}, {:.4}", lat, lon).as_str(),
                    )),
            );
        }
    }

    if let Some(h) = heading {
        let needle = format!(
            "width: 8px; height: 42px; display: flex; flex-direction: column; transform: \
             rotate({:.1}deg);",
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

    map_container = map_container.with_child(
        Dom::create_div()
            .with_css(ATTRIB)
            .with_child(Dom::create_span_with_text(attribution_text.as_str())),
    );

    Dom::create_body()
        .with_css(ROOT)
        .with_child(header)
        .with_child(map_container)
}

extern "C" fn on_zoom_in(mut data: RefAny, _info: CallbackInfo) -> Update {
    if std::env::var("AZ_MAP_DEBUG").is_ok() {
        eprintln!("[map-demo] on_zoom_in FIRED");
    }
    if let Some(mut s) = data.downcast_mut::<MapState>() {
        s.zoom_in();
    }
    Update::RefreshDom
}

extern "C" fn on_viewport_changed(
    mut data: RefAny,
    _info: CallbackInfo,
    vp: MapViewport,
) -> Update {
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
    let fix = info
        .get_location_fix()
        .into_option()
        .map(|f| (f.latitude_deg, f.longitude_deg));
    if let Some(mut s) = data.downcast_mut::<MapState>() {
        s.toggle_locate();
        s.last_fix = fix;
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

extern "C" fn on_pin_tap(mut data: RefAny, info: CallbackInfo, coord: MapLatLon) -> Update {
    if let Some(mut s) = data.downcast_mut::<MapState>() {
        s.pins.push((coord.lat_deg, coord.lon_deg));
        if let Some(rect) = info.get_hit_node_rect().into_option() {
            s.view_px = Some((rect.size.width, rect.size.height));
        }
    }
    Update::RefreshDom
}

fn cardinal(deg: f32) -> &'static str {
    const DIRS: [&str; 8] = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
    DIRS[((((deg + 22.5) % 360.0) / 45.0) as usize) % 8]
}

extern "C" fn compass_tick(mut data: RefAny, info: TimerCallbackInfo) -> TimerCallbackReturn {
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

extern "C" fn startup(data: RefAny, mut info: CallbackInfo) -> Update {
    info.add_timer(
        TimerId::unique(),
        Timer::create(
            data.clone(),
            compass_tick,
            info.get_system_time_fn(),
        ),
    );
    Update::DoNothing
}

pub fn start() {
    let data = RefAny::new(MapState::new());
    let config = AppConfig::create();
    let app = App::create(data, config);
    let mut window = WindowCreateOptions::create(layout);
    window.create_callback = Some(Callback::create(startup)).into();
    app.run(window);
}

#[cfg(target_os = "android")]
#[ctor::ctor]
fn azul_android_init() {
    start();
}
