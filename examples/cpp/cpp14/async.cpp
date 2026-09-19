#include "azul14.hpp"
#include <cstdio>

using namespace azul;

struct MapState {
    ffi::MapViewport viewport;
    HttpClient tiles;
    ThreadPool workers;
};

ffi::Update on_zoom_in(ffi::RefAny data, ffi::CallbackInfo info);
ffi::Update on_zoom_out(ffi::RefAny data, ffi::CallbackInfo info);

static auto label(const char* text, const char* css) -> Dom {
    return Dom::create_span_with_text(String(text)).with_css(String(css));
}

static auto zoom_button(const char* glyph, const char* name, RefAny data, AzCallbackType callback) -> Dom {
    return label(glyph,
        "width: 30px; height: 30px; line-height: 30px; text-align: center; "
        "background: white; color: #333333; border: 1px solid #b0b0b0; "
        "border-radius: 6px; margin-right: 6px; font-size: 18px; cursor: pointer;")
        .with_callback(AzEventFilter_hover(AzHoverEventFilter_MouseUp), std::move(data), callback)
        .with_accessibility_info(AccessibilityInfo::named(String(name), AzAccessibilityRole_PushButton));
}

static auto change_zoom(ffi::RefAny data, float delta) -> ffi::Update {
    RefAny data_wrapper(data);
    auto m = data_wrapper.downcast_mut<MapState>();
    if (!m) return Update::DoNothing;
    float zoom = m->viewport.zoom + delta;
    if (zoom < 1.0f) zoom = 1.0f;
    if (zoom > 14.0f) zoom = 14.0f;
    m->viewport.zoom = zoom;
    return Update::RefreshDom;
}

auto on_zoom_in(ffi::RefAny data, ffi::CallbackInfo info) -> ffi::Update { return change_zoom(data, 1.0f); }
auto on_zoom_out(ffi::RefAny data, ffi::CallbackInfo info) -> ffi::Update { return change_zoom(data, -1.0f); }

auto on_map_mount(ffi::RefAny data, ffi::CallbackInfo info, ffi::MapSetup setup) -> ffi::MapSetup {
    RefAny data_wrapper(data);
    MapSetup result(setup);
    auto m = data_wrapper.downcast_ref<MapState>();
    if (!m) return result.release();
    return result
        .with_http_client(m->tiles.clone())
        .with_thread_pool(m->workers.clone())
        .with_max_in_flight(8)
        .release();
}

auto layout(ffi::RefAny data, ffi::LayoutCallbackInfo info) -> ffi::Dom {
    RefAny data_wrapper(data);
    auto m = data_wrapper.downcast_ref<MapState>();
    if (!m) return Dom::create_body();
    ffi::MapViewport viewport = m->viewport;

    MapTileLayer layer = MapTileLayer::default_();
    String credit(AzString_clone(&layer.inner().attribution));

    Dom map = MapWidget::create(std::move(layer))
        .with_theme(AzMapTheme_System)
        .with_viewport(viewport)
        .with_on_mount(data_wrapper.clone(), on_map_mount)
        .dom()
        .with_css(String("width: 100%; height: 100%;"));

    char zoom_text[64];
    std::snprintf(zoom_text, sizeof(zoom_text), "vector tiles over HTTPS   -   zoom %.0f", (double)viewport.zoom);

    Dom header = Dom::create_div()
        .with_css(String("display: flex; flex-direction: row; align-items: center; "
                         "padding: 10px 14px; background: #2f3b4f; color: white;"))
        .with_child(label("Azul Maps", "font-size: 17px; font-weight: bold; margin-right: 14px;"))
        .with_child(label(zoom_text, "font-size: 12px; color: #c7d0dc;"));

    Dom controls = Dom::create_div()
        .with_css(String("position: absolute; left: 12px; top: 12px; display: flex; flex-direction: row;"))
        .with_child(zoom_button("+", "Zoom in", data_wrapper.clone(), on_zoom_in))
        .with_child(zoom_button("-", "Zoom out", data_wrapper.clone(), on_zoom_out));

    Dom frame = Dom::create_div()
        .with_css(String("flex-grow: 1; margin: 12px; border-radius: 14px; overflow: hidden; "
                         "border: 1px solid #c3cad4; background: #dfe5ec; position: relative;"))
        .with_child(std::move(map))
        .with_child(std::move(controls));

    Dom footer = Dom::create_span_with_text(std::move(credit))
        .with_css(String("padding: 6px 14px; background: #f7f9fb; border-top: 1px solid #d3d9e2; "
                         "color: #55606e; font-size: 11px;"));

    return Dom::create_body()
        .with_css(String("display: flex; flex-direction: column; height: 100%; margin: 0; padding: 0; "
                         "background: #eef1f5; font-family: sans-serif;"))
        .with_child(std::move(header))
        .with_child(std::move(frame))
        .with_child(std::move(footer));
}

int main() {
    ffi::MapViewport viewport = MapViewport::default_().release();
    viewport.centre_lat_deg = 48.2082;
    viewport.centre_lon_deg = 16.3738;
    viewport.zoom = 6.0f;
    viewport.bearing_deg = 0.0f;
    viewport.pitch_deg = 0.0f;

    MapState model = {
        viewport,
        HttpClient::create(HttpClientConfig::create()),
        ThreadPool::create(4),
    };
    RefAny data = RefAny::create(std::move(model));

    WindowCreateOptions window = WindowCreateOptions::create(layout);
    window.inner().window_state.title = az_string_from_literal("Azul Maps");
    window.inner().window_state.size.dimensions.width = 900.0f;
    window.inner().window_state.size.dimensions.height = 620.0f;

    App app = App::create(std::move(data), AppConfig::create());
    app.run(std::move(window));
    return 0;
}
