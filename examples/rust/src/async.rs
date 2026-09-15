use azul::{
    callbacks::CallbackType,
    dom::{AccessibilityInfo, AccessibilityRole, MapMountCallback},
    http::{HttpClient, HttpClientConfig},
    option::OptionRefAny,
    prelude::*,
    task::ThreadPool,
    widgets::{MapSetup, MapTileLayer, MapViewport, MapWidget},
    window::MapTheme,
};

struct MapState {
    viewport: MapViewport,
    tiles: HttpClient,
    workers: ThreadPool,
}

const ZOOM_BUTTON: &str = "width: 30px; height: 30px; line-height: 30px; text-align: center; \
                           background: white; color: #333333; border: 1px solid #b0b0b0; \
                           border-radius: 6px; margin-right: 6px; font-size: 18px; cursor: pointer;";

fn zoom_button(glyph: &str, name: &str, data: RefAny, callback: CallbackType) -> Dom {
    Dom::create_span_with_text(glyph)
        .with_css(ZOOM_BUTTON)
        .with_callback(EventFilter::Hover(HoverEventFilter::MouseUp), data, callback)
        .with_accessibility_info(AccessibilityInfo::named(name, AccessibilityRole::PushButton))
}

fn change_zoom(data: &mut RefAny, delta: f32) -> Update {
    let Some(mut state) = data.downcast_mut::<MapState>() else {
        return Update::DoNothing;
    };
    state.viewport.zoom = (state.viewport.zoom + delta).clamp(1.0, 14.0);
    Update::RefreshDom
}

extern "C" fn on_zoom_in(mut data: RefAny, _: CallbackInfo) -> Update {
    change_zoom(&mut data, 1.0)
}

extern "C" fn on_zoom_out(mut data: RefAny, _: CallbackInfo) -> Update {
    change_zoom(&mut data, -1.0)
}

extern "C" fn on_map_mount(mut data: RefAny, _: CallbackInfo, setup: MapSetup) -> MapSetup {
    let Some(state) = data.downcast_ref::<MapState>() else {
        return setup;
    };
    setup
        .with_http_client(state.tiles.clone())
        .with_thread_pool(state.workers.clone())
        .with_max_in_flight(8)
}

extern "C" fn layout(mut data: RefAny, _: LayoutCallbackInfo) -> Dom {
    let viewport = match data.downcast_ref::<MapState>() {
        Some(state) => state.viewport,
        None => return Dom::create_body(),
    };

    let layer = MapTileLayer::default();
    let credit = layer.attribution.clone();

    let map = MapWidget::create(layer)
        .with_theme(MapTheme::System)
        .with_viewport(viewport)
        .with_on_mount(
            data.clone(),
            MapMountCallback {
                cb: on_map_mount,
                callable: OptionRefAny::None,
            },
        )
        .dom()
        .with_css("width: 100%; height: 100%;");

    let header = Dom::create_div()
        .with_css(
            "display: flex; flex-direction: row; align-items: center; \
             padding: 10px 14px; background: #2f3b4f; color: white;",
        )
        .with_child(
            Dom::create_span_with_text("Azul Maps")
                .with_css("font-size: 17px; font-weight: bold; margin-right: 14px;"),
        )
        .with_child(
            Dom::create_span_with_text(
                format!("vector tiles over HTTPS   -   zoom {:.0}", viewport.zoom).as_str(),
            )
            .with_css("font-size: 12px; color: #c7d0dc;"),
        );

    let controls = Dom::create_div()
        .with_css("position: absolute; left: 12px; top: 12px; display: flex; flex-direction: row;")
        .with_child(zoom_button("+", "Zoom in", data.clone(), on_zoom_in))
        .with_child(zoom_button("-", "Zoom out", data.clone(), on_zoom_out));

    let frame = Dom::create_div()
        .with_css(
            "flex-grow: 1; margin: 12px; border-radius: 14px; overflow: hidden; \
             border: 1px solid #c3cad4; background: #dfe5ec; position: relative;",
        )
        .with_child(map)
        .with_child(controls);

    let footer = Dom::create_span_with_text(credit).with_css(
        "padding: 6px 14px; background: #f7f9fb; border-top: 1px solid #d3d9e2; \
         color: #55606e; font-size: 11px;",
    );

    Dom::create_body()
        .with_css(
            "display: flex; flex-direction: column; height: 100%; margin: 0; padding: 0; \
             background: #eef1f5; font-family: sans-serif;",
        )
        .with_child(header)
        .with_child(frame)
        .with_child(footer)
}

fn main() {
    let mut viewport = MapViewport::default();
    viewport.centre_lat_deg = 48.2082;
    viewport.centre_lon_deg = 16.3738;
    viewport.zoom = 6.0;
    viewport.bearing_deg = 0.0;
    viewport.pitch_deg = 0.0;

    let state = MapState {
        viewport,
        tiles: HttpClient::create(HttpClientConfig::create()),
        workers: ThreadPool::create(4),
    };

    let mut window = WindowCreateOptions::create(layout);
    window.window_state.title = "Azul Maps".into();
    window.window_state.size.dimensions.width = 900.0;
    window.window_state.size.dimensions.height = 620.0;

    let app = App::create(RefAny::new(state), AppConfig::create());
    app.run(window);
}
