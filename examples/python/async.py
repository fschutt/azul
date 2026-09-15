from azul import *

ZOOM_BUTTON = ("width: 30px; height: 30px; line-height: 30px; text-align: center; "
               "background: white; color: #333333; border: 1px solid #b0b0b0; "
               "border-radius: 6px; margin-right: 6px; font-size: 18px; cursor: pointer;")


class MapState:
    def __init__(self):
        self.viewport = MapViewport.default()
        self.viewport.centre_lat_deg = 48.2082
        self.viewport.centre_lon_deg = 16.3738
        self.viewport.zoom = 6.0
        self.viewport.bearing_deg = 0.0
        self.viewport.pitch_deg = 0.0
        self.tiles = HttpClient.create(HttpClientConfig.create())
        self.workers = ThreadPool.create(4)


def label(text, css):
    return Dom.create_span_with_text(text).with_css(css)


def zoom_button(glyph, name, data, callback):
    return (label(glyph, ZOOM_BUTTON)
            .with_callback(EventFilter.Hover(HoverEventFilter.MouseUp), data, callback)
            .with_accessibility_info(AccessibilityInfo.named(name, AccessibilityRole.PushButton)))


def change_zoom(data, delta):
    data.viewport.zoom = max(1.0, min(14.0, data.viewport.zoom + delta))
    return Update.RefreshDom


def on_zoom_in(data, info):
    return change_zoom(data, 1.0)


def on_zoom_out(data, info):
    return change_zoom(data, -1.0)


def on_map_mount(data, info, setup):
    return (setup.with_http_client(data.tiles)
                 .with_thread_pool(data.workers)
                 .with_max_in_flight(8))


def layout(data, info):
    layer = MapTileLayer.default()
    credit = layer.attribution

    map_dom = (MapWidget.create(layer)
               .with_theme(MapTheme.System)
               .with_viewport(data.viewport)
               .with_on_mount(data, on_map_mount)
               .dom()
               .with_css("width: 100%; height: 100%;"))

    header = (Dom.create_div()
              .with_css("display: flex; flex-direction: row; align-items: center; "
                        "padding: 10px 14px; background: #2f3b4f; color: white;")
              .with_child(label("Azul Maps", "font-size: 17px; font-weight: bold; margin-right: 14px;"))
              .with_child(label("vector tiles over HTTPS   -   zoom %.0f" % data.viewport.zoom,
                                "font-size: 12px; color: #c7d0dc;")))

    controls = (Dom.create_div()
                .with_css("position: absolute; left: 12px; top: 12px; display: flex; flex-direction: row;")
                .with_child(zoom_button("+", "Zoom in", data, on_zoom_in))
                .with_child(zoom_button("-", "Zoom out", data, on_zoom_out)))

    frame = (Dom.create_div()
             .with_css("flex-grow: 1; margin: 12px; border-radius: 14px; overflow: hidden; "
                       "border: 1px solid #c3cad4; background: #dfe5ec; position: relative;")
             .with_child(map_dom)
             .with_child(controls))

    footer = label(credit, "padding: 6px 14px; background: #f7f9fb; border-top: 1px solid #d3d9e2; "
                           "color: #55606e; font-size: 11px;")

    return (Dom.create_body()
            .with_css("display: flex; flex-direction: column; height: 100%; margin: 0; padding: 0; "
                      "background: #eef1f5; font-family: sans-serif;")
            .with_child(header)
            .with_child(frame)
            .with_child(footer))


if __name__ == "__main__":
    window = WindowCreateOptions.create(layout)
    state = window.window_state
    state.title = "Azul Maps"
    size = state.size
    dimensions = size.dimensions
    dimensions.width = 900.0
    dimensions.height = 620.0
    size.dimensions = dimensions
    state.size = size
    window.window_state = state

    app = App.create(MapState(), AppConfig.create())
    app.run(window)
