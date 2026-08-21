#include "azul.h"
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#define TILE_PX      256.0f
#define CELLS        8
#define CELL_PX      (TILE_PX / (float)CELLS)
#define CACHE_SLOTS  256
#define HEADER_PX    44.0f
#define FOOTER_PX    22.0f
#define FETCH_MS     140
// Slice the simulated latency so a terminate request is noticed promptly.
#define TERMINATE_POLL_MS 20

typedef struct {
    uint64_t key;
    uint8_t  state;
    uint8_t  cells[CELLS * CELLS];
} TileSlot;

typedef struct {
    double   centre_x;
    double   centre_y;
    int      zoom;
    bool     dragging;
    float    drag_x;
    float    drag_y;
    int      loaded;
    int      pending;
    TileSlot cache[CACHE_SLOTS];
} MapState;

void MapState_destructor(void* s) { }
AZ_REFLECT(MapState, MapState_destructor);

typedef struct {
    uint64_t key;
    int      zoom;
    int      tile_x;
    int      tile_y;
} TileRequest;

void TileRequest_destructor(void* s) { }
AZ_REFLECT(TileRequest, TileRequest_destructor);

typedef struct {
    uint64_t key;
    uint8_t  cells[CELLS * CELLS];
} TileReady;

void TileReady_destructor(void* s) { }
AZ_REFLECT(TileReady, TileReady_destructor);

static const char* TERRAIN[8] = {
    "#8fbcd4", "#aad3df", "#efe6c9", "#f2efe9",
    "#e3ddd5", "#cdebb0", "#a8d18d", "#ffffff"
};

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info);
AzUpdate on_window_created(AzRefAny data, AzCallbackInfo info);
AzUpdate on_pointer_down(AzRefAny data, AzCallbackInfo info);
AzUpdate on_pointer_move(AzRefAny data, AzCallbackInfo info);
AzUpdate on_pointer_up(AzRefAny data, AzCallbackInfo info);
AzUpdate on_zoom_in(AzRefAny data, AzCallbackInfo info);
AzUpdate on_zoom_out(AzRefAny data, AzCallbackInfo info);
void tile_worker(AzRefAny initial_data, AzThreadSender sender, AzThreadReceiver recv);
AzUpdate tile_writeback(AzRefAny app_data, AzRefAny incoming, AzCallbackInfo info);

static AzString str(const char* s) {
    return AzString_copyFromBytes((const uint8_t*)s, 0, strlen(s));
}

static AzDom div(const char* css) {
    AzDom d = AzDom_createDiv();
    AzDom_setCss(&d, str(css));
    return d;
}

static AzDom label(const char* text, const char* css) {
    AzDom d = AzDom_createDivWithText(str(text));
    AzDom_setCss(&d, str(css));
    return d;
}

static float lattice(int x, int y) {
    uint32_t h = (uint32_t)x * 374761393u + (uint32_t)y * 668265263u;
    h = (h ^ (h >> 13)) * 1274126177u;
    h ^= h >> 16;
    return (float)(h & 0xFFFFFFu) / (float)0xFFFFFFu;
}

static float value_noise(float x, float y) {
    int x0 = (int)floorf(x);
    int y0 = (int)floorf(y);
    float fx = x - (float)x0;
    float fy = y - (float)y0;
    fx = fx * fx * (3.0f - 2.0f * fx);
    fy = fy * fy * (3.0f - 2.0f * fy);
    float a = lattice(x0, y0);
    float b = lattice(x0 + 1, y0);
    float c = lattice(x0, y0 + 1);
    float d = lattice(x0 + 1, y0 + 1);
    return (a * (1.0f - fx) + b * fx) * (1.0f - fy) + (c * (1.0f - fx) + d * fx) * fy;
}

static uint8_t terrain_at(float u, float v) {
    float t = 0.62f * value_noise(u * 0.18f, v * 0.18f)
            + 0.28f * value_noise(u * 0.55f, v * 0.55f)
            + 0.10f * value_noise(u * 1.70f, v * 1.70f);
    if (t < 0.38f) return 0;
    if (t < 0.46f) return 1;
    if (t < 0.49f) return 2;
    if (t < 0.60f) return 3;
    if (t < 0.66f) return 4;
    if (t < 0.76f) return 5;
    return 6;
}

static void world_coords(int zoom, int tile_x, int tile_y, int i, int j, float* u, float* v) {
    float span = 64.0f / (float)(1 << zoom);
    *u = ((float)tile_x + (float)i / (float)CELLS) * span;
    *v = ((float)tile_y + (float)j / (float)CELLS) * span;
}

static uint8_t cell_terrain(int zoom, int tile_x, int tile_y, int i, int j) {
    float u, v;
    world_coords(zoom, tile_x, tile_y, i, j, &u, &v);
    uint8_t t = terrain_at(u, v);
    if (t <= 1) {
        return t;
    }
    int gx = tile_x * CELLS + i;
    int gy = tile_y * CELLS + j;
    if (gx % 9 == 4 || gy % 11 == 6) {
        return 7;
    }
    return t;
}

static uint64_t tile_key(int zoom, int tile_x, int tile_y) {
    return ((uint64_t)(uint32_t)zoom << 48)
         | ((uint64_t)(uint32_t)tile_x << 24)
         | (uint64_t)(uint32_t)tile_y;
}

static size_t slot_of(uint64_t key) {
    return (size_t)((key * 1181783497276652981ull) >> 56) % CACHE_SLOTS;
}

static int world_tiles(int zoom) {
    return 1 << zoom;
}

static int wrap_tile(int value, int count) {
    int m = value % count;
    return (m < 0) ? m + count : m;
}

static void visible_range(double centre_x, double centre_y, int zoom,
                          float view_w, float view_h,
                          float* origin_x, float* origin_y,
                          int* first_x, int* first_y, int* cols, int* rows) {
    float world_px = TILE_PX * (float)world_tiles(zoom);
    float left = (float)(centre_x * world_px) - view_w * 0.5f;
    float top = (float)(centre_y * world_px) - view_h * 0.5f;
    *first_x = (int)floorf(left / TILE_PX);
    *first_y = (int)floorf(top / TILE_PX);
    *origin_x = (float)(*first_x) * TILE_PX - left;
    *origin_y = (float)(*first_y) * TILE_PX - top;
    *cols = (int)ceilf(view_w / TILE_PX) + 1;
    *rows = (int)ceilf(view_h / TILE_PX) + 1;
}

static void request_visible_tiles(AzRefAny data, AzCallbackInfo* info, float view_w, float view_h) {
    MapStateRefMut m = MapStateRefMut_create(&data);
    if (!MapState_downcastMut(&data, &m)) {
        AzRefAny_delete(&data);
        return;
    }

    float ox, oy;
    int first_x, first_y, cols, rows;
    visible_range(m.ptr->centre_x, m.ptr->centre_y, m.ptr->zoom, view_w, view_h,
                  &ox, &oy, &first_x, &first_y, &cols, &rows);

    int count = world_tiles(m.ptr->zoom);
    int zoom = m.ptr->zoom;

    TileRequest wanted[64];
    size_t wanted_len = 0;

    for (int cy = 0; cy < rows; cy++) {
        int ty = first_y + cy;
        if (ty < 0 || ty >= count) continue;
        for (int cx = 0; cx < cols; cx++) {
            int tx = wrap_tile(first_x + cx, count);
            uint64_t key = tile_key(zoom, tx, ty);
            size_t slot = slot_of(key);
            if (m.ptr->cache[slot].key == key && m.ptr->cache[slot].state != 0) {
                continue;
            }
            if (wanted_len >= 64) continue;
            m.ptr->cache[slot].key = key;
            m.ptr->cache[slot].state = 1;
            m.ptr->pending += 1;
            wanted[wanted_len].key = key;
            wanted[wanted_len].zoom = zoom;
            wanted[wanted_len].tile_x = tx;
            wanted[wanted_len].tile_y = ty;
            wanted_len += 1;
        }
    }

    MapStateRefMut_delete(&m);

    for (size_t i = 0; i < wanted_len; i++) {
        AzRefAny init = TileRequest_upcast(wanted[i]);
        AzThread thread = AzThread_create(init, AzRefAny_clone(&data), tile_worker);
        AzCallbackInfo_addThread(info, AzThreadId_unique(), thread);
    }

    AzRefAny_delete(&data);
}

void tile_worker(AzRefAny initial_data, AzThreadSender sender, AzThreadReceiver recv) {
    TileRequestRef req = TileRequestRef_create(&initial_data);
    if (!TileRequest_downcastRef(&initial_data, &req)) {
        return;
    }

    uint64_t key = req.ptr->key;
    int zoom = req.ptr->zoom;
    int tile_x = req.ptr->tile_x;
    int tile_y = req.ptr->tile_y;
    TileRequestRef_delete(&req);

    // Sleep in slices, checking for TerminateThread between them. A single
    // sleep is uninterruptible: at shutdown the worker cannot acknowledge
    // within the join grace period, so the framework detaches it instead of
    // joining, and ThreadSanitizer reports a thread leak in pthread_create.
    // One tile is cheap; sixteen sleeping at once is what makes it visible.
    for (int waited = 0; waited < FETCH_MS; waited += TERMINATE_POLL_MS) {
        AzThread_sleepMs(TERMINATE_POLL_MS);
        AzOptionThreadSendMsg early = AzThreadReceiver_recv(&recv);
        if (early.None.tag == AzOptionThreadSendMsg_Tag_Some
            && early.Some.payload.TerminateThread.tag == AzThreadSendMsg_Tag_TerminateThread) {
            return;
        }
    }

    TileReady ready;
    ready.key = key;
    for (int j = 0; j < CELLS; j++) {
        for (int i = 0; i < CELLS; i++) {
            ready.cells[j * CELLS + i] = cell_terrain(zoom, tile_x, tile_y, i, j);
        }
    }

    AzWriteBackCallback wb = { .cb = tile_writeback, .ctx = AzOptionRefAny_none() };
    AzThreadWriteBackMsg wb_msg = { .refany = TileReady_upcast(ready), .callback = wb };
    AzThreadSender_send(&sender, AzThreadReceiveMsg_writeBack(wb_msg));
}

AzUpdate tile_writeback(AzRefAny app_data, AzRefAny incoming, AzCallbackInfo info) {
    MapStateRefMut m = MapStateRefMut_create(&app_data);
    if (!MapState_downcastMut(&app_data, &m)) {
        return AzUpdate_DoNothing;
    }

    TileReadyRef ready = TileReadyRef_create(&incoming);
    if (!TileReady_downcastRef(&incoming, &ready)) {
        MapStateRefMut_delete(&m);
        return AzUpdate_DoNothing;
    }

    size_t slot = slot_of(ready.ptr->key);
    if (m.ptr->cache[slot].key == ready.ptr->key && m.ptr->cache[slot].state == 1) {
        memcpy(m.ptr->cache[slot].cells, ready.ptr->cells, sizeof(ready.ptr->cells));
        m.ptr->cache[slot].state = 2;
        m.ptr->loaded += 1;
        if (m.ptr->pending > 0) m.ptr->pending -= 1;
    }

    TileReadyRef_delete(&ready);
    MapStateRefMut_delete(&m);
    return AzUpdate_RefreshDom;
}

static void view_size(AzCallbackInfo* info, float* w, float* h) {
    AzFullWindowState state = AzCallbackInfo_getCurrentWindowState(info);
    *w = state.size.dimensions.width;
    *h = state.size.dimensions.height - HEADER_PX - FOOTER_PX;
    if (*h < TILE_PX) *h = TILE_PX;
}

AzUpdate on_window_created(AzRefAny data, AzCallbackInfo info) {
    float w, h;
    view_size(&info, &w, &h);
    request_visible_tiles(AzRefAny_clone(&data), &info, w, h);
    return AzUpdate_RefreshDom;
}

AzUpdate on_pointer_down(AzRefAny data, AzCallbackInfo info) {
    AzOptionLogicalPosition cursor = AzCallbackInfo_getCursorRelativeToViewport(&info);
    if (cursor.None.tag != AzOptionLogicalPosition_Tag_Some) {
        return AzUpdate_DoNothing;
    }
    MapStateRefMut m = MapStateRefMut_create(&data);
    if (!MapState_downcastMut(&data, &m)) {
        return AzUpdate_DoNothing;
    }
    m.ptr->dragging = true;
    m.ptr->drag_x = cursor.Some.payload.x;
    m.ptr->drag_y = cursor.Some.payload.y;
    MapStateRefMut_delete(&m);
    return AzUpdate_DoNothing;
}

AzUpdate on_pointer_move(AzRefAny data, AzCallbackInfo info) {
    AzOptionLogicalPosition cursor = AzCallbackInfo_getCursorRelativeToViewport(&info);
    if (cursor.None.tag != AzOptionLogicalPosition_Tag_Some) {
        return AzUpdate_DoNothing;
    }
    AzMouseState mouse = AzCallbackInfo_getCurrentMouseState(&info);

    MapStateRefMut m = MapStateRefMut_create(&data);
    if (!MapState_downcastMut(&data, &m)) {
        return AzUpdate_DoNothing;
    }
    if (!m.ptr->dragging || !mouse.left_down) {
        m.ptr->dragging = mouse.left_down && m.ptr->dragging;
        MapStateRefMut_delete(&m);
        return AzUpdate_DoNothing;
    }

    float dx = cursor.Some.payload.x - m.ptr->drag_x;
    float dy = cursor.Some.payload.y - m.ptr->drag_y;
    m.ptr->drag_x = cursor.Some.payload.x;
    m.ptr->drag_y = cursor.Some.payload.y;

    double world_px = (double)TILE_PX * (double)world_tiles(m.ptr->zoom);
    m.ptr->centre_x -= (double)dx / world_px;
    m.ptr->centre_y -= (double)dy / world_px;
    if (m.ptr->centre_x < 0.0) m.ptr->centre_x += 1.0;
    if (m.ptr->centre_x > 1.0) m.ptr->centre_x -= 1.0;
    if (m.ptr->centre_y < 0.05) m.ptr->centre_y = 0.05;
    if (m.ptr->centre_y > 0.95) m.ptr->centre_y = 0.95;
    MapStateRefMut_delete(&m);

    float w, h;
    view_size(&info, &w, &h);
    request_visible_tiles(AzRefAny_clone(&data), &info, w, h);
    return AzUpdate_RefreshDom;
}

AzUpdate on_pointer_up(AzRefAny data, AzCallbackInfo info) {
    MapStateRefMut m = MapStateRefMut_create(&data);
    if (!MapState_downcastMut(&data, &m)) {
        return AzUpdate_DoNothing;
    }
    m.ptr->dragging = false;
    MapStateRefMut_delete(&m);
    return AzUpdate_DoNothing;
}

static AzUpdate change_zoom(AzRefAny data, AzCallbackInfo* info, int delta) {
    MapStateRefMut m = MapStateRefMut_create(&data);
    if (!MapState_downcastMut(&data, &m)) {
        return AzUpdate_DoNothing;
    }
    int zoom = m.ptr->zoom + delta;
    if (zoom < 1) zoom = 1;
    if (zoom > 12) zoom = 12;
    m.ptr->zoom = zoom;
    MapStateRefMut_delete(&m);

    float w, h;
    view_size(info, &w, &h);
    request_visible_tiles(AzRefAny_clone(&data), info, w, h);
    return AzUpdate_RefreshDom;
}

AzUpdate on_zoom_in(AzRefAny data, AzCallbackInfo info) {
    return change_zoom(data, &info, 1);
}

AzUpdate on_zoom_out(AzRefAny data, AzCallbackInfo info) {
    return change_zoom(data, &info, -1);
}

static AzDom tile_dom(const TileSlot* slot, int zoom, int tile_x, int tile_y,
                      float left, float top) {
    char css[256];
    snprintf(css, sizeof(css),
        "position: absolute; left: %.0fpx; top: %.0fpx; width: %.0fpx; height: %.0fpx; "
        "overflow: hidden;", left, top, TILE_PX, TILE_PX);
    AzDom tile = div(css);

    if (slot == NULL || slot->state != 2) {
        float u, v;
        world_coords(zoom, tile_x, tile_y, CELLS / 2, CELLS / 2, &u, &v);
        snprintf(css, sizeof(css),
            "width: 100%%; height: 100%%; background: %s; opacity: 0.45;",
            TERRAIN[terrain_at(u, v)]);
        AzDom_addChild(&tile, div(css));
        return tile;
    }

    for (int j = 0; j < CELLS; j++) {
        snprintf(css, sizeof(css),
            "display: flex; flex-direction: row; height: %.2fpx;", CELL_PX);
        AzDom row = div(css);
        for (int i = 0; i < CELLS; i++) {
            snprintf(css, sizeof(css),
                "width: %.2fpx; height: %.2fpx; background: %s;",
                CELL_PX, CELL_PX, TERRAIN[slot->cells[j * CELLS + i]]);
            AzDom_addChild(&row, div(css));
        }
        AzDom_addChild(&tile, row);
    }
    return tile;
}

static AzDom zoom_button(const char* glyph, AzRefAny data, AzCallbackType cb) {
    AzDom b = label(glyph,
        "width: 28px; height: 28px; line-height: 28px; text-align: center; "
        "background: white; color: #333333; border: 1px solid #b0b0b0; "
        "margin-right: 4px; font-size: 18px; cursor: pointer;");
    AzDom_addCallback(&b, AzEventFilter_hover(AzHoverEventFilter_mouseUp()), data, cb);
    return b;
}

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    MapStateRef m = MapStateRef_create(&data);
    if (!MapState_downcastRef(&data, &m)) {
        return AzDom_createBody();
    }

    double centre_x = m.ptr->centre_x;
    double centre_y = m.ptr->centre_y;
    int zoom = m.ptr->zoom;
    int loaded = m.ptr->loaded;
    int pending = m.ptr->pending;

    float view_w = AzLayoutCallbackInfo_getWindowWidth(&info);
    float view_h = AzLayoutCallbackInfo_getWindowHeight(&info) - HEADER_PX - FOOTER_PX;
    if (view_h < TILE_PX) view_h = TILE_PX;

    float ox, oy;
    int first_x, first_y, cols, rows;
    visible_range(centre_x, centre_y, zoom, view_w, view_h,
                  &ox, &oy, &first_x, &first_y, &cols, &rows);

    AzDom canvas = div("position: absolute; left: 0; top: 0; right: 0; bottom: 0;");

    int count = world_tiles(zoom);
    for (int cy = 0; cy < rows; cy++) {
        int ty = first_y + cy;
        if (ty < 0 || ty >= count) continue;
        for (int cx = 0; cx < cols; cx++) {
            int tx = wrap_tile(first_x + cx, count);
            uint64_t key = tile_key(zoom, tx, ty);
            size_t slot = slot_of(key);
            const TileSlot* entry = (m.ptr->cache[slot].key == key) ? &m.ptr->cache[slot] : NULL;
            AzDom_addChild(&canvas, tile_dom(entry, zoom, tx, ty,
                ox + (float)cx * TILE_PX, oy + (float)cy * TILE_PX));
        }
    }

    AzDom marker = div(
        "position: absolute; left: 50%; top: 50%; width: 14px; height: 14px; "
        "margin-left: -7px; margin-top: -7px; border-radius: 7px; "
        "background: #d7263d; border: 2px solid white;");

    char css[192];
    snprintf(css, sizeof(css),
        "position: absolute; left: 0; top: 0; right: 0; bottom: 0; overflow: hidden; "
        "background: #dfe6ec; cursor: grab;");
    AzDom map_area = div(css);
    AzDom_addCallback(&map_area, AzEventFilter_hover(AzHoverEventFilter_mouseDown()),
                      AzRefAny_clone(&data), on_pointer_down);
    AzDom_addCallback(&map_area, AzEventFilter_hover(AzHoverEventFilter_mouseOver()),
                      AzRefAny_clone(&data), on_pointer_move);
    AzDom_addCallback(&map_area, AzEventFilter_hover(AzHoverEventFilter_mouseUp()),
                      AzRefAny_clone(&data), on_pointer_up);
    AzDom_addChild(&map_area, canvas);
    AzDom_addChild(&map_area, marker);

    AzDom controls = div(
        "position: absolute; left: 12px; top: 12px; display: flex; flex-direction: row;");
    AzDom_addChild(&controls, zoom_button("+", AzRefAny_clone(&data), on_zoom_in));
    AzDom_addChild(&controls, zoom_button("-", AzRefAny_clone(&data), on_zoom_out));
    AzDom_addChild(&map_area, controls);

    char status[128];
    snprintf(status, sizeof(status), "zoom %d   %d tiles loaded   %d in flight",
             zoom, loaded, pending);

    AzDom header = div(
        "display: flex; flex-direction: row; align-items: center; height: 44px; "
        "padding-left: 14px; padding-right: 14px; background: #24303f; color: white;");
    AzDom_addChild(&header, label("Azul Maps",
        "font-size: 16px; font-weight: bold; margin-right: 16px;"));
    AzDom_addChild(&header, label(status, "font-size: 12px; color: #9fb0c4;"));

    AzDom footer = label("drag to pan   -   tiles are generated on worker threads",
        "height: 22px; line-height: 22px; padding-left: 14px; background: #f3f5f7; "
        "color: #5b6875; font-size: 11px; border-top: 1px solid #d3d9df;");

    AzDom stage = div("position: relative; flex-grow: 1;");
    AzDom_addChild(&stage, map_area);

    AzDom body = AzDom_createBody();
    AzDom_setCss(&body, str(
        "display: flex; flex-direction: column; height: 100%; margin: 0; padding: 0; "
        "font-family: sans-serif;"));
    AzDom_addChild(&body, header);
    AzDom_addChild(&body, stage);
    AzDom_addChild(&body, footer);

    MapStateRef_delete(&m);
    return body;
}

int main(void) {
    MapState model;
    memset(&model, 0, sizeof(model));
    model.centre_x = 0.5234;
    model.centre_y = 0.3391;
    model.zoom = 6;

    AzRefAny data = MapState_upcast(model);

    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    window.create_callback = AzOptionCallback_some(AzCallback_create(on_window_created));
    window.window_state.title = str("Azul Maps");
    window.window_state.size.dimensions.width = 900.0;
    window.window_state.size.dimensions.height = 620.0;

    AzAppConfig config = AzAppConfig_create();
    AzApp app = AzApp_create(data, config);
    AzApp_run(&app, window);
    AzApp_delete(&app);
    return 0;
}
