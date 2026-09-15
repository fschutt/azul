#include "azul03.hpp"
#include <cstdio>

using namespace azul;

struct MapState {
    AzMapViewport viewport;
    HttpClient tiles;
    ThreadPool workers;
};
AZ_REFLECT(MapState);

AzUpdate on_zoom_in(AzRefAny data, AzCallbackInfo info);
AzUpdate on_zoom_out(AzRefAny data, AzCallbackInfo info);

static Dom label(const char* text, const char* css) {
    Dom d = Dom::create_span_with_text(String(text));
    d.set_css(String(css));
    return d;
}

static Dom zoom_button(const char* glyph, const char* name, RefAny data, AzCallbackType callback) {
    Dom b = label(glyph,
        "width: 30px; height: 30px; line-height: 30px; text-align: center; "
        "background: white; color: #333333; border: 1px solid #b0b0b0; "
        "border-radius: 6px; margin-right: 6px; font-size: 18px; cursor: pointer;");
    b.add_callback(AzEventFilter_hover(AzHoverEventFilter_MouseUp), data, callback);
    return b.with_accessibility_info(AccessibilityInfo::named(String(name), AzAccessibilityRole_PushButton));
}

static AzUpdate change_zoom(AzRefAny data, float delta) {
    RefAny data_wrapper(data);
    MapState* m = MapState_downcast_mut(data_wrapper);
    if (!m) return AzUpdate_DoNothing;
    float zoom = m->viewport.zoom + delta;
    if (zoom < 1.0f) zoom = 1.0f;
    if (zoom > 14.0f) zoom = 14.0f;
    m->viewport.zoom = zoom;
    return AzUpdate_RefreshDom;
}

AzUpdate on_zoom_in(AzRefAny data, AzCallbackInfo info) { (void)info; return change_zoom(data, 1.0f); }
AzUpdate on_zoom_out(AzRefAny data, AzCallbackInfo info) { (void)info; return change_zoom(data, -1.0f); }

AzMapSetup on_map_mount(AzRefAny data, AzCallbackInfo info, AzMapSetup setup) {
    (void)info;
    RefAny data_wrapper(data);
    MapSetup result(setup);
    const MapState* m = MapState_downcast_ref(data_wrapper);
    if (!m) return result.release();
    result = result.with_http_client(m->tiles.clone());
    result = result.with_thread_pool(m->workers.clone());
    result = result.with_max_in_flight(8);
    return result.release();
}

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    (void)info;
    RefAny data_wrapper(data);
    const MapState* m = MapState_downcast_ref(data_wrapper);
    if (!m) return AzDom_createBody();
    AzMapViewport viewport = m->viewport;

    MapTileLayer layer = MapTileLayer::default_();
    String credit(AzString_clone(&layer.inner().attribution));

    MapWidget widget = MapWidget::create(layer);
    widget = widget.with_theme(AzMapTheme_System);
    widget = widget.with_viewport(viewport);
    widget = widget.with_on_mount(data_wrapper.clone(), on_map_mount);
    Dom map = widget.dom();
    map.set_css(String("width: 100%; height: 100%;"));

    char zoom_text[64];
    std::snprintf(zoom_text, sizeof(zoom_text), "vector tiles over HTTPS   -   zoom %.0f", (double)viewport.zoom);

    Dom header = Dom::create_div();
    header.set_css(String("display: flex; flex-direction: row; align-items: center; "
                          "padding: 10px 14px; background: #2f3b4f; color: white;"));
    header.add_child(label("Azul Maps", "font-size: 17px; font-weight: bold; margin-right: 14px;"));
    header.add_child(label(zoom_text, "font-size: 12px; color: #c7d0dc;"));

    Dom controls = Dom::create_div();
    controls.set_css(String("position: absolute; left: 12px; top: 12px; display: flex; flex-direction: row;"));
    controls.add_child(zoom_button("+", "Zoom in", data_wrapper.clone(), on_zoom_in));
    controls.add_child(zoom_button("-", "Zoom out", data_wrapper.clone(), on_zoom_out));

    Dom frame = Dom::create_div();
    frame.set_css(String("flex-grow: 1; margin: 12px; border-radius: 14px; overflow: hidden; "
                         "border: 1px solid #c3cad4; background: #dfe5ec; position: relative;"));
    frame.add_child(map);
    frame.add_child(controls);

    Dom footer = Dom::create_span_with_text(credit);
    footer.set_css(String("padding: 6px 14px; background: #f7f9fb; border-top: 1px solid #d3d9e2; "
                          "color: #55606e; font-size: 11px;"));

    Dom body = Dom::create_body();
    body.set_css(String("display: flex; flex-direction: column; height: 100%; margin: 0; padding: 0; "
                        "background: #eef1f5; font-family: sans-serif;"));
    body.add_child(header);
    body.add_child(frame);
    body.add_child(footer);
    return body.release();
}

int main() {
    MapState model = {
        MapViewport::default_().release(),
        HttpClient::create(HttpClientConfig::create()),
        ThreadPool::create(4)
    };
    model.viewport.centre_lat_deg = 48.2082;
    model.viewport.centre_lon_deg = 16.3738;
    model.viewport.zoom = 6.0f;
    model.viewport.bearing_deg = 0.0f;
    model.viewport.pitch_deg = 0.0f;
    RefAny data = MapState_upcast(model);

    WindowCreateOptions window = WindowCreateOptions::create(layout);
    window.inner().window_state.title = az_string_from_literal("Azul Maps");
    window.inner().window_state.size.dimensions.width = 900.0f;
    window.inner().window_state.size.dimensions.height = 620.0f;

    App app = App::create(data, AppConfig::default_());
    app.run(window);
    return 0;
}
