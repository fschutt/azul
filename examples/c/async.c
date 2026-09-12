/*
 * Azul Maps - the map widget's flow, end to end, in C.
 *
 * A real slippy map: `MapWidget` fetches OpenStreetMap VECTOR tiles
 * (Mapbox Vector Tile / `.pbf`) over HTTPS on background threads, decodes
 * them, styles them with MapCSS and rasterises them - none of which this
 * file has to implement. What the example shows is the FLOW an application
 * actually writes:
 *
 *   1. describe the tile source          -> AzMapTileLayer_default()
 *   2. build the widget from it          -> AzMapWidget_create(layer)
 *   3. pick a look and a camera          -> withTheme / withViewport
 *   4. hand back a Dom that self-fetches -> AzMapWidget_domWithFetch(w)
 *
 * Step 4 is what makes it asynchronous: the returned Dom registers the
 * built-in tile worker, so the first frame paints immediately with whatever
 * tiles are cached and later frames fill in as HTTP responses land. The UI
 * thread never blocks on the network.
 *
 * It also shows CLIP SHAPES: the map sits in a rounded container with
 * `overflow: hidden`, so the tiles are clipped to the rounded corners
 * rather than squaring them off.
 */
#ifndef _WIN32
#define _POSIX_C_SOURCE 200809L
#endif
#include "azul.h"
#include <stdio.h>
#include <string.h>

/* The camera is the ONLY state an app this size needs: the widget owns the
 * tile cache, the worker threads and the decoded geometry. */
typedef struct {
    AzMapViewport viewport;
} MapState;

void MapState_destructor(void* s) { (void)s; }
AZ_REFLECT(MapState, MapState_destructor);

AzUpdate on_zoom_in(AzRefAny data, AzCallbackInfo info);
AzUpdate on_zoom_out(AzRefAny data, AzCallbackInfo info);

static AzString str(const char* s) {
    return AzString_copyFromBytes((const uint8_t*)s, 0, strlen(s));
}

static AzDom div(const char* css) {
    AzDom d = AzDom_createDiv();
    AzDom_setCss(&d, str(css));
    return d;
}

static AzDom label(const char* text, const char* css) {
    AzDom d = AzDom_createSpanWithText(str(text));
    AzDom_setCss(&d, str(css));
    return d;
}

static AzDom zoom_button(const char* glyph, AzRefAny data, AzCallbackType cb) {
    AzDom b = label(glyph,
        "width: 30px; height: 30px; line-height: 30px; text-align: center; "
        "background: white; color: #333333; border: 1px solid #b0b0b0; "
        "border-radius: 6px; margin-right: 6px; font-size: 18px; cursor: pointer;");
    AzDom_addCallback(&b, AzEventFilter_hover(AzHoverEventFilter_mouseUp()), data, cb);
    return b;
}

static AzUpdate change_zoom(AzRefAny data, float delta) {
    MapStateRefMut m = MapStateRefMut_create(&data);
    if (!MapState_downcastMut(&data, &m)) {
        return AzUpdate_DoNothing;
    }
    float zoom = m.ptr->viewport.zoom + delta;
    /* The default layer carries min_zoom 0 / max_zoom 14; going past that
     * just asks the server for tiles that do not exist. */
    if (zoom < 1.0f)  zoom = 1.0f;
    if (zoom > 14.0f) zoom = 14.0f;
    m.ptr->viewport.zoom = zoom;
    MapStateRefMut_delete(&m);
    return AzUpdate_RefreshDom;
}

AzUpdate on_zoom_in(AzRefAny data, AzCallbackInfo info)  { (void)info; return change_zoom(data,  1.0f); }
AzUpdate on_zoom_out(AzRefAny data, AzCallbackInfo info) { (void)info; return change_zoom(data, -1.0f); }

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    (void)info;
    MapStateRef m = MapStateRef_create(&data);
    if (!MapState_downcastRef(&data, &m)) {
        return AzDom_createBody();
    }
    AzMapViewport viewport = m.ptr->viewport;
    MapStateRef_delete(&m);

    /* 1. The tile source. The default is OpenFreeMap's public planet vector
     *    tiles - real OSM data, no API key - and carries the attribution the
     *    licence requires. Point `url_template` somewhere else for your own. */
    AzMapTileLayer layer = AzMapTileLayer_default();
    AzString credit = AzString_clone(&layer.attribution);

    /* 2-3. The widget, its theme and its camera. A theme is a cartography
     *      (Positron, Bright, Liberty, Google, Apple) with a light and a dark
     *      MapCSS palette; the window's light/dark theme picks the half and
     *      the visible tiles are re-decoded when it changes. `System` is the
     *      platform's family (Apple here). */
    AzMapWidget widget = AzMapWidget_create(layer);
    widget = AzMapWidget_withTheme(widget, AzMapTheme_System);
    widget = AzMapWidget_withViewport(widget, viewport);

    /* 4. `domWithFetch` wires the built-in HTTP + MVT worker to this Dom.
     *    Tiles arrive on background threads and land in later frames. */
    AzDom map = AzMapWidget_domWithFetch(widget);
    AzDom_setCss(&map, str("width: 100%; height: 100%;"));

    char text[128];

    AzDom body = AzDom_createBody();
    AzDom_setCss(&body, str(
        "display: flex; flex-direction: column; height: 100%; margin: 0; padding: 0; "
        "background: #eef1f5; font-family: sans-serif;"));

    AzDom header = div(
        "display: flex; flex-direction: row; align-items: center; "
        "padding: 10px 14px; background: #2f3b4f; color: white;");
    AzDom_addChild(&header, label("Azul Maps",
        "font-size: 17px; font-weight: bold; margin-right: 14px;"));
    snprintf(text, sizeof(text), "vector tiles over HTTPS   -   zoom %.0f", (double)viewport.zoom);
    AzDom_addChild(&header, label(text, "font-size: 12px; color: #c7d0dc;"));
    AzDom_addChild(&body, header);

    /* CLIP SHAPES: the rounded, overflow-hidden frame clips the tiles to its
     * corners. Everything the widget paints is confined to this shape. */
    AzDom frame = div(
        "flex-grow: 1; margin: 12px; border-radius: 14px; overflow: hidden; "
        "border: 1px solid #c3cad4; background: #dfe5ec; position: relative;");
    AzDom_addChild(&frame, map);

    AzDom controls = div(
        "position: absolute; left: 12px; top: 12px; display: flex; flex-direction: row;");
    AzDom_addChild(&controls, zoom_button("+", AzRefAny_clone(&data), on_zoom_in));
    AzDom_addChild(&controls, zoom_button("-", AzRefAny_clone(&data), on_zoom_out));
    AzDom_addChild(&frame, controls);
    AzDom_addChild(&body, frame);

    /* The licence requires the attribution to be visible. */
    AzDom footer = AzDom_createSpanWithText(credit);
    AzDom_setCss(&footer, str(
        "padding: 6px 14px; background: #f7f9fb; border-top: 1px solid #d3d9e2; "
        "color: #55606e; font-size: 11px;"));
    AzDom_addChild(&body, footer);

    return body;
}

int main(void) {
    MapState model;
    memset(&model, 0, sizeof(model));
    /* Central Europe, wide enough that coastlines and motorways both show. */
    model.viewport.centre_lat_deg = 48.2082;
    model.viewport.centre_lon_deg = 16.3738;
    model.viewport.zoom = 6.0f;
    model.viewport.bearing_deg = 0.0f;
    model.viewport.pitch_deg = 0.0f;

    AzRefAny data = MapState_upcast(model);

    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    window.window_state.title = str("Azul Maps");
    window.window_state.size.dimensions.width = 900.0;
    window.window_state.size.dimensions.height = 620.0;

    AzAppConfig config = AzAppConfig_create();
    AzApp app = AzApp_create(data, config);
    AzApp_run(&app, window);
    AzApp_delete(&app);
    return 0;
}
