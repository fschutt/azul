use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use azul::prelude::*;

const TILE_PX: f32 = 256.0;
const CELLS: usize = 8;
const CELL_PX: f32 = TILE_PX / CELLS as f32;
const HEADER_PX: f32 = 44.0;
const FOOTER_PX: f32 = 22.0;
const FETCH_MS: u64 = 140;

const TERRAIN: &[&str] = &[
    "#8fbcd4", "#aad3df", "#efe6c9", "#f2efe9", "#e3ddd5", "#cdebb0", "#a8d18d", "#ffffff",
];

type TileKey = (u32, u32, u32);

#[derive(Clone, Copy, PartialEq)]
enum TileState {
    Fetching,
    Ready,
}

struct MapState {
    centre_x: f64,
    centre_y: f64,
    zoom: u32,
    dragging: bool,
    drag: LogicalPosition,
    tiles: Arc<Mutex<BTreeMap<TileKey, (TileState, Vec<u8>)>>>,
}

struct TileRequest {
    key: TileKey,
    tiles: Arc<Mutex<BTreeMap<TileKey, (TileState, Vec<u8>)>>>,
}

struct TileReady;

fn lattice(x: i32, y: i32) -> f32 {
    let mut h = (x as u32)
        .wrapping_mul(374_761_393)
        .wrapping_add((y as u32).wrapping_mul(668_265_263));
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    h ^= h >> 16;
    (h & 0x00FF_FFFF) as f32 / 0x00FF_FFFF as f32
}

fn value_noise(x: f32, y: f32) -> f32 {
    let x0 = x.floor();
    let y0 = y.floor();
    let fx = {
        let t = x - x0;
        t * t * (3.0 - 2.0 * t)
    };
    let fy = {
        let t = y - y0;
        t * t * (3.0 - 2.0 * t)
    };
    let (xi, yi) = (x0 as i32, y0 as i32);
    let a = lattice(xi, yi);
    let b = lattice(xi + 1, yi);
    let c = lattice(xi, yi + 1);
    let d = lattice(xi + 1, yi + 1);
    (a * (1.0 - fx) + b * fx) * (1.0 - fy) + (c * (1.0 - fx) + d * fx) * fy
}

fn terrain_at(u: f32, v: f32) -> u8 {
    let t = 0.62 * value_noise(u * 0.18, v * 0.18)
        + 0.28 * value_noise(u * 0.55, v * 0.55)
        + 0.10 * value_noise(u * 1.70, v * 1.70);
    match t {
        t if t < 0.38 => 0,
        t if t < 0.46 => 1,
        t if t < 0.49 => 2,
        t if t < 0.60 => 3,
        t if t < 0.66 => 4,
        t if t < 0.76 => 5,
        _ => 6,
    }
}

fn world_coords(zoom: u32, tile_x: u32, tile_y: u32, i: usize, j: usize) -> (f32, f32) {
    let span = 64.0 / (1u32 << zoom) as f32;
    (
        (tile_x as f32 + i as f32 / CELLS as f32) * span,
        (tile_y as f32 + j as f32 / CELLS as f32) * span,
    )
}

fn cell_terrain(zoom: u32, tile_x: u32, tile_y: u32, i: usize, j: usize) -> u8 {
    let (u, v) = world_coords(zoom, tile_x, tile_y, i, j);
    let t = terrain_at(u, v);
    if t <= 1 {
        return t;
    }
    let gx = tile_x as usize * CELLS + i;
    let gy = tile_y as usize * CELLS + j;
    if gx % 9 == 4 || gy % 11 == 6 {
        7
    } else {
        t
    }
}

fn world_tiles(zoom: u32) -> i64 {
    1i64 << zoom
}

fn visible_range(
    centre_x: f64,
    centre_y: f64,
    zoom: u32,
    view_w: f32,
    view_h: f32,
) -> (f32, f32, i64, i64, i64, i64) {
    let world_px = TILE_PX as f64 * world_tiles(zoom) as f64;
    let left = centre_x * world_px - view_w as f64 * 0.5;
    let top = centre_y * world_px - view_h as f64 * 0.5;
    let first_x = (left / TILE_PX as f64).floor() as i64;
    let first_y = (top / TILE_PX as f64).floor() as i64;
    (
        (first_x as f64 * TILE_PX as f64 - left) as f32,
        (first_y as f64 * TILE_PX as f64 - top) as f32,
        first_x,
        first_y,
        (view_w / TILE_PX).ceil() as i64 + 1,
        (view_h / TILE_PX).ceil() as i64 + 1,
    )
}

fn request_visible_tiles(data: &mut RefAny, info: &mut CallbackInfo, view_w: f32, view_h: f32) {
    let (wanted, tiles) = {
        let state = match data.downcast_ref::<MapState>() {
            Some(s) => s,
            None => return,
        };
        let (_, _, first_x, first_y, cols, rows) =
            visible_range(state.centre_x, state.centre_y, state.zoom, view_w, view_h);
        let count = world_tiles(state.zoom);
        let mut cache = state.tiles.lock().unwrap();
        let mut wanted = Vec::new();
        for cy in 0..rows {
            let ty = first_y + cy;
            if ty < 0 || ty >= count {
                continue;
            }
            for cx in 0..cols {
                let tx = (first_x + cx).rem_euclid(count);
                let key = (state.zoom, tx as u32, ty as u32);
                if cache.contains_key(&key) {
                    continue;
                }
                cache.insert(key, (TileState::Fetching, Vec::new()));
                wanted.push(key);
            }
        }
        (wanted, state.tiles.clone())
    };

    for key in wanted {
        let init = RefAny::new(TileRequest {
            key,
            tiles: tiles.clone(),
        });
        info.add_thread(
            ThreadId::unique(),
            Thread::create(init, data.clone(), tile_worker),
        );
    }
}

extern "C" fn tile_worker(mut init: RefAny, mut sender: ThreadSender, mut recv: ThreadReceiver) {
    let (key, tiles) = match init.downcast_ref::<TileRequest>() {
        Some(r) => (r.key, r.tiles.clone()),
        None => return,
    };

    std::thread::sleep(std::time::Duration::from_millis(FETCH_MS));

    if matches!(
        recv.recv().into_option(),
        Some(ThreadSendMsg::TerminateThread)
    ) {
        return;
    }

    let (zoom, tile_x, tile_y) = key;
    let mut cells = Vec::with_capacity(CELLS * CELLS);
    for j in 0..CELLS {
        for i in 0..CELLS {
            cells.push(cell_terrain(zoom, tile_x, tile_y, i, j));
        }
    }

    if let Ok(mut cache) = tiles.lock() {
        cache.insert(key, (TileState::Ready, cells));
    }

    sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg {
        refany: RefAny::new(TileReady),
        callback: WriteBackCallback {
            cb: tile_writeback,
            ctx: OptionRefAny::None,
        },
    }));
}

extern "C" fn tile_writeback(_: RefAny, _: RefAny, _: CallbackInfo) -> Update {
    Update::RefreshDom
}

fn view_size(info: &CallbackInfo) -> (f32, f32) {
    let dims = info.get_current_window_state().size.dimensions;
    (
        dims.width,
        (dims.height - HEADER_PX - FOOTER_PX).max(TILE_PX),
    )
}

extern "C" fn on_window_created(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let (w, h) = view_size(&info);
    request_visible_tiles(&mut data, &mut info, w, h);
    Update::RefreshDom
}

extern "C" fn on_pointer_down(mut data: RefAny, info: CallbackInfo) -> Update {
    let cursor = match info.get_cursor_relative_to_viewport().into_option() {
        Some(p) => p,
        None => return Update::DoNothing,
    };
    if let Some(mut state) = data.downcast_mut::<MapState>() {
        state.dragging = true;
        state.drag = cursor;
    }
    Update::DoNothing
}

extern "C" fn on_pointer_move(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let cursor = match info.get_cursor_relative_to_viewport().into_option() {
        Some(p) => p,
        None => return Update::DoNothing,
    };
    let left_down = info.get_current_mouse_state().left_down;

    {
        let mut state = match data.downcast_mut::<MapState>() {
            Some(s) => s,
            None => return Update::DoNothing,
        };
        if !state.dragging || !left_down {
            state.dragging = state.dragging && left_down;
            return Update::DoNothing;
        }
        let dx = cursor.x - state.drag.x;
        let dy = cursor.y - state.drag.y;
        state.drag = cursor;

        let world_px = TILE_PX as f64 * world_tiles(state.zoom) as f64;
        state.centre_x = (state.centre_x - dx as f64 / world_px).rem_euclid(1.0);
        state.centre_y = (state.centre_y - dy as f64 / world_px).clamp(0.05, 0.95);
    }

    let (w, h) = view_size(&info);
    request_visible_tiles(&mut data, &mut info, w, h);
    Update::RefreshDom
}

extern "C" fn on_pointer_up(mut data: RefAny, _: CallbackInfo) -> Update {
    if let Some(mut state) = data.downcast_mut::<MapState>() {
        state.dragging = false;
    }
    Update::DoNothing
}

fn change_zoom(data: &mut RefAny, info: &mut CallbackInfo, delta: i32) -> Update {
    {
        let mut state = match data.downcast_mut::<MapState>() {
            Some(s) => s,
            None => return Update::DoNothing,
        };
        state.zoom = (state.zoom as i32 + delta).clamp(1, 12) as u32;
    }
    let (w, h) = view_size(info);
    request_visible_tiles(data, info, w, h);
    Update::RefreshDom
}

extern "C" fn on_zoom_in(mut data: RefAny, mut info: CallbackInfo) -> Update {
    change_zoom(&mut data, &mut info, 1)
}

extern "C" fn on_zoom_out(mut data: RefAny, mut info: CallbackInfo) -> Update {
    change_zoom(&mut data, &mut info, -1)
}

fn tile_dom(
    entry: Option<&(TileState, Vec<u8>)>,
    zoom: u32,
    tile_x: u32,
    tile_y: u32,
    left: f32,
    top: f32,
) -> Dom {
    let mut tile = Dom::create_div().with_css(format!(
        "position: absolute; left: {left:.0}px; top: {top:.0}px; \
         width: {TILE_PX:.0}px; height: {TILE_PX:.0}px; overflow: hidden;"
    ));

    let cells = match entry {
        Some((TileState::Ready, cells)) => cells,
        _ => {
            let (u, v) = world_coords(zoom, tile_x, tile_y, CELLS / 2, CELLS / 2);
            let colour = TERRAIN[terrain_at(u, v) as usize];
            tile.add_child(Dom::create_div().with_css(format!(
                "width: 100%; height: 100%; background: {colour}; opacity: 0.45;"
            )));
            return tile;
        }
    };

    for j in 0..CELLS {
        let mut row = Dom::create_div().with_css(format!(
            "display: flex; flex-direction: row; height: {CELL_PX:.2}px;"
        ));
        for i in 0..CELLS {
            let colour = TERRAIN[cells[j * CELLS + i] as usize];
            row.add_child(Dom::create_div().with_css(format!(
                "width: {CELL_PX:.2}px; height: {CELL_PX:.2}px; background: {colour};"
            )));
        }
        tile.add_child(row);
    }
    tile
}

fn zoom_button(
    glyph: &str,
    data: RefAny,
    cb: extern "C" fn(RefAny, CallbackInfo) -> Update,
) -> Dom {
    Dom::create_div_with_text(glyph)
        .with_css(
            "width: 28px; height: 28px; line-height: 28px; text-align: center; background: white; \
             color: #333333; border: 1px solid #b0b0b0; margin-right: 4px; font-size: 18px; \
             cursor: pointer;",
        )
        .with_callback(EventFilter::Hover(HoverEventFilter::MouseUp), data, cb)
}

extern "C" fn layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    let (centre_x, centre_y, zoom, tiles) = {
        let state = match data.downcast_ref::<MapState>() {
            Some(s) => s,
            None => return Dom::create_body(),
        };
        (
            state.centre_x,
            state.centre_y,
            state.zoom,
            state.tiles.clone(),
        )
    };

    let view_w = info.get_window_width();
    let view_h = (info.get_window_height() - HEADER_PX - FOOTER_PX).max(TILE_PX);
    let (ox, oy, first_x, first_y, cols, rows) =
        visible_range(centre_x, centre_y, zoom, view_w, view_h);

    let cache = tiles.lock().unwrap();
    let loaded = cache
        .values()
        .filter(|(s, _)| *s == TileState::Ready)
        .count();
    let pending = cache.len() - loaded;

    let mut canvas =
        Dom::create_div().with_css("position: absolute; left: 0; top: 0; right: 0; bottom: 0;");
    let count = world_tiles(zoom);
    for cy in 0..rows {
        let ty = first_y + cy;
        if ty < 0 || ty >= count {
            continue;
        }
        for cx in 0..cols {
            let tx = (first_x + cx).rem_euclid(count);
            let key = (zoom, tx as u32, ty as u32);
            canvas.add_child(tile_dom(
                cache.get(&key),
                zoom,
                tx as u32,
                ty as u32,
                ox + cx as f32 * TILE_PX,
                oy + cy as f32 * TILE_PX,
            ));
        }
    }
    drop(cache);

    let marker = Dom::create_div().with_css(
        "position: absolute; left: 50%; top: 50%; width: 14px; height: 14px; margin-left: -7px; \
         margin-top: -7px; border-radius: 7px; background: #d7263d; border: 2px solid white;",
    );

    let controls = Dom::create_div()
        .with_css("position: absolute; left: 12px; top: 12px; display: flex; flex-direction: row;")
        .with_child(zoom_button("+", data.clone(), on_zoom_in))
        .with_child(zoom_button("-", data.clone(), on_zoom_out));

    let map_area = Dom::create_div()
        .with_css(
            "position: absolute; left: 0; top: 0; right: 0; bottom: 0; overflow: hidden; \
             background: #dfe6ec; cursor: grab;",
        )
        .with_callback(
            EventFilter::Hover(HoverEventFilter::MouseDown),
            data.clone(),
            on_pointer_down,
        )
        .with_callback(
            EventFilter::Hover(HoverEventFilter::MouseOver),
            data.clone(),
            on_pointer_move,
        )
        .with_callback(
            EventFilter::Hover(HoverEventFilter::MouseUp),
            data.clone(),
            on_pointer_up,
        )
        .with_child(canvas)
        .with_child(marker)
        .with_child(controls);

    let header = Dom::create_div()
        .with_css(
            "display: flex; flex-direction: row; align-items: center; height: 44px; \
             padding-left: 14px; padding-right: 14px; background: #24303f; color: white;",
        )
        .with_child(
            Dom::create_div_with_text("Azul Maps")
                .with_css("font-size: 16px; font-weight: bold; margin-right: 16px;"),
        )
        .with_child(
            Dom::create_div_with_text(format!(
                "zoom {zoom}   {loaded} tiles loaded   {pending} in flight"
            ))
            .with_css("font-size: 12px; color: #9fb0c4;"),
        );

    let footer =
        Dom::create_div_with_text("drag to pan   -   tiles are generated on worker threads")
            .with_css(
                "height: 22px; line-height: 22px; padding-left: 14px; background: #f3f5f7; \
             color: #5b6875; font-size: 11px; border-top: 1px solid #d3d9df;",
            );

    let stage = Dom::create_div()
        .with_css("position: relative; flex-grow: 1;")
        .with_child(map_area);

    Dom::create_body()
        .with_css(
            "display: flex; flex-direction: column; height: 100%; margin: 0; padding: 0; \
             font-family: sans-serif;",
        )
        .with_child(header)
        .with_child(stage)
        .with_child(footer)
}

fn main() {
    let data = RefAny::new(MapState {
        centre_x: 0.5234,
        centre_y: 0.3391,
        zoom: 6,
        dragging: false,
        drag: LogicalPosition::create(0.0, 0.0),
        tiles: Arc::new(Mutex::new(BTreeMap::new())),
    });

    let app = App::create(data, AppConfig::create());
    let mut window = WindowCreateOptions::create(layout);
    window.create_callback = Some(Callback::create(on_window_created)).into();
    window.window_state.title = "Azul Maps".into();
    window.window_state.size.dimensions.width = 900.0;
    window.window_state.size.dimensions.height = 620.0;
    app.run(window);
}
