from azul import *

TILE_PX = 256
CELLS = 8
CELL_PX = TILE_PX // CELLS
COLS = 4
ROWS = 3

TERRAIN = ["#8fbcd4", "#aad3df", "#efe6c9", "#f2efe9",
           "#e3ddd5", "#cdebb0", "#a8d18d", "#ffffff"]

CLICK = EventFilter.Hover(HoverEventFilter.MouseUp)


class MapState:
    def __init__(self):
        self.tile_x = 21
        self.tile_y = 24
        self.zoom = 6


def lattice(x, y):
    h = (x * 374761393 + y * 668265263) & 0xFFFFFFFF
    h = ((h ^ (h >> 13)) * 1274126177) & 0xFFFFFFFF
    h ^= h >> 16
    return (h & 0xFFFFFF) / float(0xFFFFFF)


def value_noise(x, y):
    x0 = int(x // 1)
    y0 = int(y // 1)
    fx = x - x0
    fy = y - y0
    fx = fx * fx * (3.0 - 2.0 * fx)
    fy = fy * fy * (3.0 - 2.0 * fy)
    a = lattice(x0, y0)
    b = lattice(x0 + 1, y0)
    c = lattice(x0, y0 + 1)
    d = lattice(x0 + 1, y0 + 1)
    return (a * (1 - fx) + b * fx) * (1 - fy) + (c * (1 - fx) + d * fx) * fy


def terrain_at(u, v):
    t = (0.62 * value_noise(u * 0.18, v * 0.18)
         + 0.28 * value_noise(u * 0.55, v * 0.55)
         + 0.10 * value_noise(u * 1.70, v * 1.70))
    for limit, index in ((0.38, 0), (0.46, 1), (0.49, 2), (0.60, 3), (0.66, 4), (0.76, 5)):
        if t < limit:
            return index
    return 6


def cell_terrain(zoom, tile_x, tile_y, i, j):
    span = 64.0 / (1 << zoom)
    t = terrain_at((tile_x + i / float(CELLS)) * span, (tile_y + j / float(CELLS)) * span)
    if t <= 1:
        return t
    if (tile_x * CELLS + i) % 9 == 4 or (tile_y * CELLS + j) % 11 == 6:
        return 7
    return t


def tile_dom(zoom, tile_x, tile_y):
    tile = Dom.create_div().with_css(
        "width:%dpx;height:%dpx;overflow:hidden;" % (TILE_PX, TILE_PX))
    for j in range(CELLS):
        row = Dom.create_div().with_css(
            "display:flex;flex-direction:row;height:%dpx;" % CELL_PX)
        for i in range(CELLS):
            colour = TERRAIN[cell_terrain(zoom, tile_x, tile_y, i, j)]
            row = row.with_child(Dom.create_div().with_css(
                "width:%dpx;height:%dpx;background:%s;" % (CELL_PX, CELL_PX, colour)))
        tile = tile.with_child(row)
    return tile


def control(text, data, callback):
    return (Dom.create_div()
            .with_css("width:28px;height:28px;line-height:28px;text-align:center;"
                      "background:white;color:#333333;border:1px solid #b0b0b0;"
                      "margin-right:4px;font-size:16px;cursor:pointer;")
            .with_child(Dom.create_p_with_text(text))
            .with_callback(CLICK, data, callback))


def pan(data, dx, dy):
    count = 1 << data.zoom
    data.tile_x = (data.tile_x + dx) % count
    data.tile_y = max(0, min(count - ROWS, data.tile_y + dy))
    return Update.RefreshDom


def on_west(data, info):
    return pan(data, -1, 0)


def on_east(data, info):
    return pan(data, 1, 0)


def on_north(data, info):
    return pan(data, 0, -1)


def on_south(data, info):
    return pan(data, 0, 1)


def on_zoom_in(data, info):
    if data.zoom >= 12:
        return Update.DoNothing
    data.zoom += 1
    data.tile_x *= 2
    data.tile_y *= 2
    return Update.RefreshDom


def on_zoom_out(data, info):
    if data.zoom <= 1:
        return Update.DoNothing
    data.zoom -= 1
    data.tile_x //= 2
    data.tile_y //= 2
    return Update.RefreshDom


def layout(data, info):
    grid = Dom.create_div().with_css("display:flex;flex-direction:column;")
    count = 1 << data.zoom
    for row in range(ROWS):
        strip = Dom.create_div().with_css("display:flex;flex-direction:row;")
        for col in range(COLS):
            strip = strip.with_child(
                tile_dom(data.zoom, (data.tile_x + col) % count, data.tile_y + row))
        grid = grid.with_child(strip)

    controls = (Dom.create_div()
                .with_css("position:absolute;left:12px;top:12px;display:flex;flex-direction:row;")
                .with_child(control("+", data, on_zoom_in))
                .with_child(control("-", data, on_zoom_out))
                .with_child(control("<", data, on_west))
                .with_child(control(">", data, on_east))
                .with_child(control("^", data, on_north))
                .with_child(control("v", data, on_south)))

    stage = (Dom.create_div()
             .with_css("position:relative;flex-grow:1;overflow:hidden;background:#dfe6ec;")
             .with_child(grid)
             .with_child(controls))

    header = (Dom.create_div()
              .with_css("display:flex;flex-direction:row;align-items:center;height:44px;"
                        "padding-left:14px;background:#24303f;color:white;")
              .with_child(Dom.create_div()
                          .with_css("font-size:16px;font-weight:bold;margin-right:16px;")
                          .with_child(Dom.create_p_with_text(
                              "Azul Maps")))
              .with_child(Dom.create_div()
                          .with_css("font-size:12px;color:#9fb0c4;")
                          .with_child(Dom.create_p_with_text(
                              "zoom %d   tile %d/%d" % (data.zoom, data.tile_x, data.tile_y)))))

    footer = (Dom.create_div()
              .with_css("height:22px;line-height:22px;padding-left:14px;background:#f3f5f7;"
                        "color:#5b6875;font-size:11px;border-top:1px solid #d3d9df;")
              .with_child(Dom.create_p_with_text(
                  "tiles are generated procedurally - no network, no assets")))

    return (Dom.create_body()
            .with_css("display:flex;flex-direction:column;height:100%;margin:0;padding:0;"
                      "font-family:sans-serif;")
            .with_child(header)
            .with_child(stage)
            .with_child(footer))


state = MapState()
window = WindowCreateOptions.create(layout)
app = App.create(state, AppConfig.create())
app.run(window)
